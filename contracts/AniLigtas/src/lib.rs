#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, symbol_short,
    Address, Bytes, Env,
};

// ---------------------------------------------------------------------------
// Storage key types
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    Farmer(Bytes),
    Claim(Bytes),
    PoolBalance,
    Admin,
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A registered cooperative member (farmer).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FarmerRecord {
    pub wallet: Address,
    pub cooperative_id: Bytes,
    pub has_active_claim: bool,
}

/// A crop-loss disaster relief claim.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRecord {
    pub farmer_id: Bytes,
    pub loss_amount: i128,
    pub approved_amount: i128,
    /// 0 = Pending, 1 = Approved, 2 = Rejected
    pub status: u32,
    pub evidence_ref: Bytes,
}

// ---------------------------------------------------------------------------
// Error codes
// #[contracterror] generates the correct u32-backed error type Soroban expects.
// Do NOT use #[contracttype] on error enums — that caused one of the 4 errors.
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    FarmerAlreadyRegistered = 1,
    Unauthorised            = 2,
    FarmerNotFound          = 3,
    ClaimNotFound           = 4,
    ActiveClaimExists       = 5,
    InsufficientPoolFunds   = 6,
    ClaimAlreadyResolved    = 7,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct AniLigtasContract;

#[contractimpl]
impl AniLigtasContract {

    /// Deploy-time setup: store the admin (cooperative board wallet).
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::PoolBalance, &0_i128);
    }

    /// Enroll a farming household into the cooperative relief programme.
    /// Admin-only; prevents duplicate farmer IDs.
    pub fn register_farmer(
        env: Env,
        farmer_id: Bytes,
        wallet: Address,
        cooperative_id: Bytes,
    ) -> Result<(), Error> {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if env.storage().persistent().has(&DataKey::Farmer(farmer_id.clone())) {
            return Err(Error::FarmerAlreadyRegistered);
        }

        let record = FarmerRecord {
            wallet: wallet.clone(),
            cooperative_id,
            has_active_claim: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Farmer(farmer_id.clone()), &record);

        env.events().publish(
            (symbol_short!("enroll"), farmer_id),
            wallet,
        );

        Ok(())
    }

    /// Donor / NGO / LGU deposits USDC into the community relief pool.
    /// Uses soroban_sdk::token::TokenClient (correct path for SDK v20+).
    pub fn deposit_to_pool(
        env: Env,
        token_contract: Address,
        donor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        donor.require_auth();

        // TokenClient is the correct type — NOT token::Client
        let token = soroban_sdk::token::TokenClient::new(&env, &token_contract);
        token.transfer(&donor, &env.current_contract_address(), &amount);

        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PoolBalance)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::PoolBalance, &(current + amount));

        env.events().publish(
            (symbol_short!("deposit"), donor),
            amount,
        );

        Ok(())
    }

    /// Registered farmer files a crop-loss disaster relief claim.
    /// One active claim per farmer at a time to prevent system gaming.
    pub fn file_claim(
        env: Env,
        farmer_id: Bytes,
        claim_id: Bytes,
        loss_amount: i128,
        evidence_ref: Bytes,
    ) -> Result<(), Error> {
        let mut farmer: FarmerRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Farmer(farmer_id.clone()))
            .ok_or(Error::FarmerNotFound)?;

        farmer.wallet.require_auth();

        if farmer.has_active_claim {
            return Err(Error::ActiveClaimExists);
        }

        let claim = ClaimRecord {
            farmer_id: farmer_id.clone(),
            loss_amount,
            approved_amount: 0,
            status: 0, // Pending
            evidence_ref,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Claim(claim_id.clone()), &claim);

        farmer.has_active_claim = true;
        env.storage()
            .persistent()
            .set(&DataKey::Farmer(farmer_id.clone()), &farmer);

        env.events().publish(
            (symbol_short!("claim"), claim_id),
            (farmer_id, loss_amount),
        );

        Ok(())
    }

    /// Admin approves a pending claim and atomically releases USDC to the farmer.
    /// Reverts entirely if the pool is underfunded — no partial payouts.
    pub fn approve_and_disburse(
        env: Env,
        claim_id: Bytes,
        approved_amount: i128,
        token_contract: Address,
    ) -> Result<(), Error> {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut claim: ClaimRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Claim(claim_id.clone()))
            .ok_or(Error::ClaimNotFound)?;

        if claim.status != 0 {
            return Err(Error::ClaimAlreadyResolved);
        }

        let pool: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PoolBalance)
            .unwrap_or(0);

        if pool < approved_amount {
            return Err(Error::InsufficientPoolFunds);
        }

        let farmer: FarmerRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Farmer(claim.farmer_id.clone()))
            .ok_or(Error::FarmerNotFound)?;

        // Mark claim Approved
        claim.approved_amount = approved_amount;
        claim.status = 1;
        env.storage()
            .persistent()
            .set(&DataKey::Claim(claim_id.clone()), &claim);

        // Deduct from pool
        env.storage()
            .persistent()
            .set(&DataKey::PoolBalance, &(pool - approved_amount));

        // Clear active-claim flag so farmer can file again next season
        let mut farmer_mut = farmer.clone();
        farmer_mut.has_active_claim = false;
        env.storage()
            .persistent()
            .set(&DataKey::Farmer(claim.farmer_id.clone()), &farmer_mut);

        // Transfer USDC: contract → farmer wallet
        let token = soroban_sdk::token::TokenClient::new(&env, &token_contract);
        token.transfer(
            &env.current_contract_address(),
            &farmer.wallet,
            &approved_amount,
        );

        // "disburse" = 8 chars, within symbol_short! 9-char limit
        env.events().publish(
            (symbol_short!("disburse"), claim_id),
            (farmer.wallet, approved_amount),
        );

        Ok(())
    }

    /// Admin rejects a claim. Frees the farmer to refile with better evidence.
    pub fn reject_claim(env: Env, claim_id: Bytes) -> Result<(), Error> {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let mut claim: ClaimRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Claim(claim_id.clone()))
            .ok_or(Error::ClaimNotFound)?;

        if claim.status != 0 {
            return Err(Error::ClaimAlreadyResolved);
        }

        claim.status = 2; // Rejected
        env.storage()
            .persistent()
            .set(&DataKey::Claim(claim_id.clone()), &claim);

        let mut farmer: FarmerRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Farmer(claim.farmer_id.clone()))
            .ok_or(Error::FarmerNotFound)?;

        farmer.has_active_claim = false;
        env.storage()
            .persistent()
            .set(&DataKey::Farmer(claim.farmer_id), &farmer);

        env.events().publish(
            (symbol_short!("reject"), claim_id),
            true,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read-only helpers
    // -----------------------------------------------------------------------

    pub fn get_pool_balance(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::PoolBalance)
            .unwrap_or(0)
    }

    pub fn get_claim(env: Env, claim_id: Bytes) -> Result<ClaimRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Claim(claim_id))
            .ok_or(Error::ClaimNotFound)
    }

    pub fn get_farmer(env: Env, farmer_id: Bytes) -> Result<FarmerRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Farmer(farmer_id))
            .ok_or(Error::FarmerNotFound)
    }
}

#[cfg(test)]
mod test;