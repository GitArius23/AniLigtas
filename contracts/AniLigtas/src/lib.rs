#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    Address, Bytes, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Storage key types
// ---------------------------------------------------------------------------

/// All keys used in persistent contract storage.
#[contracttype]
pub enum DataKey {
    /// Maps farmer_id (Bytes) → FarmerRecord
    Farmer(Bytes),
    /// Maps claim_id (Bytes) → ClaimRecord
    Claim(Bytes),
    /// Total USDC held in the community relief pool (i128 token units)
    PoolBalance,
    /// Admin address (cooperative board or NGO deployer)
    Admin,
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A registered cooperative member (farmer).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FarmerRecord {
    /// The farmer's Stellar wallet address
    pub wallet: Address,
    /// Cooperative ID they belong to (e.g. b"IFUGAO-COOP-01")
    pub cooperative_id: Bytes,
    /// Whether this farmer has an active (unresolved) claim
    pub has_active_claim: bool,
}

/// A crop-loss disaster relief claim.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRecord {
    /// Farmer identifier linking back to FarmerRecord
    pub farmer_id: Bytes,
    /// Declared loss amount in USDC token units (e.g. 50_000_000 = $50 USDC)
    pub loss_amount: i128,
    /// Approved payout amount (set by admin during approval; 0 until approved)
    pub approved_amount: i128,
    /// Claim lifecycle: 0 = Pending, 1 = Approved, 2 = Rejected
    pub status: u32,
    /// IPFS hash or other evidence reference (e.g. photo of damaged crops)
    pub evidence_ref: Bytes,
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// Farmer ID is already registered in this cooperative
    FarmerAlreadyRegistered = 1,
    /// Caller is not the authorised admin / cooperative board
    Unauthorised = 2,
    /// Farmer ID not found in storage
    FarmerNotFound = 3,
    /// Claim ID not found in storage
    ClaimNotFound = 4,
    /// Farmer already has an active open claim
    ActiveClaimExists = 5,
    /// Community pool has insufficient funds to pay the approved amount
    InsufficientPoolFunds = 6,
    /// Claim has already been resolved (approved or rejected); cannot change
    ClaimAlreadyResolved = 7,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct AniLigtasContract;

#[contractimpl]
impl AniLigtasContract {

    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------

    /// Deploy-time setup: store the admin (cooperative board wallet).
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        // Start pool at zero; funds arrive via deposit_to_pool()
        env.storage().persistent().set(&DataKey::PoolBalance, &0_i128);
    }

    // -----------------------------------------------------------------------
    // register_farmer
    // -----------------------------------------------------------------------

    /// Enroll a farming household into the cooperative relief programme.
    ///
    /// - `farmer_id`      : unique identifier bytes (e.g. national farm ID)
    /// - `wallet`         : farmer's Stellar wallet for payout routing
    /// - `cooperative_id` : which cooperative cluster this farmer belongs to
    ///
    /// Prevents duplicate enrollments. Admin-only to avoid self-registration abuse.
    pub fn register_farmer(
        env: Env,
        farmer_id: Bytes,
        wallet: Address,
        cooperative_id: Bytes,
    ) -> Result<(), Error> {
        // Only the cooperative admin may onboard farmers
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Reject duplicate farmer IDs
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

        // Emit enrollment event for off-chain cooperative dashboard
        env.events().publish(
            (symbol_short!("enroll"), farmer_id),
            wallet,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // deposit_to_pool
    // -----------------------------------------------------------------------

    /// Donor, NGO, or LGU deposits USDC into the community relief pool.
    ///
    /// The caller transfers USDC to the contract address externally via the
    /// token contract, then calls this to record the ledger-side balance.
    ///
    /// - `token_contract` : USDC (or XLM) Stellar token contract address
    /// - `donor`          : donor's wallet (must authorise the transfer)
    /// - `amount`         : USDC amount in token base units
    pub fn deposit_to_pool(
        env: Env,
        token_contract: Address,
        donor: Address,
        amount: i128,
    ) -> Result<(), Error> {
        donor.require_auth();

        // Pull USDC from donor wallet into contract
        let token = soroban_sdk::token::Client::new(&env, &token_contract);
        token.transfer(&donor, &env.current_contract_address(), &amount);

        // Update internal pool balance tracker
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

    // -----------------------------------------------------------------------
    // file_claim
    // -----------------------------------------------------------------------

    /// A registered farmer files a crop-loss disaster relief claim.
    ///
    /// - `farmer_id`    : must already be registered
    /// - `claim_id`     : unique claim reference (e.g. UUID bytes)
    /// - `loss_amount`  : estimated USDC value of crop losses
    /// - `evidence_ref` : IPFS CID or URL hash pointing to damage photos / barangay cert
    ///
    /// One active claim per farmer at a time to prevent gaming the system.
    pub fn file_claim(
        env: Env,
        farmer_id: Bytes,
        claim_id: Bytes,
        loss_amount: i128,
        evidence_ref: Bytes,
    ) -> Result<(), Error> {
        // Retrieve farmer — must be enrolled
        let mut farmer: FarmerRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Farmer(farmer_id.clone()))
            .ok_or(Error::FarmerNotFound)?;

        // Farmer's wallet must authorise filing their own claim
        farmer.wallet.require_auth();

        // Block filing a second claim while one is pending
        if farmer.has_active_claim {
            return Err(Error::ActiveClaimExists);
        }

        let claim = ClaimRecord {
            farmer_id: farmer_id.clone(),
            loss_amount,
            approved_amount: 0,
            status: 0, // Pending
            evidence_ref: evidence_ref.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Claim(claim_id.clone()), &claim);

        // Flag farmer as having an active claim to prevent duplicates
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

    // -----------------------------------------------------------------------
    // approve_and_disburse
    // -----------------------------------------------------------------------

    /// Admin reviews a pending claim and releases USDC payout from the pool.
    ///
    /// - `claim_id`        : claim to approve
    /// - `approved_amount` : USDC payout (may be less than declared loss)
    /// - `token_contract`  : USDC Stellar token contract address
    ///
    /// Atomically: marks claim Approved → deducts pool → transfers USDC to farmer.
    /// If pool is underfunded, the whole transaction reverts — no partial payouts.
    pub fn approve_and_disburse(
        env: Env,
        claim_id: Bytes,
        approved_amount: i128,
        token_contract: Address,
    ) -> Result<(), Error> {
        // Admin-only action
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        // Load the claim
        let mut claim: ClaimRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Claim(claim_id.clone()))
            .ok_or(Error::ClaimNotFound)?;

        // Refuse to re-process a resolved claim
        if claim.status != 0 {
            return Err(Error::ClaimAlreadyResolved);
        }

        // Verify pool has enough funds before committing anything
        let pool: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PoolBalance)
            .unwrap_or(0);
        if pool < approved_amount {
            return Err(Error::InsufficientPoolFunds);
        }

        // Retrieve farmer wallet for payout
        let farmer: FarmerRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Farmer(claim.farmer_id.clone()))
            .ok_or(Error::FarmerNotFound)?;

        // Update claim state
        claim.approved_amount = approved_amount;
        claim.status = 1; // Approved
        env.storage()
            .persistent()
            .set(&DataKey::Claim(claim_id.clone()), &claim);

        // Deduct from pool balance
        env.storage()
            .persistent()
            .set(&DataKey::PoolBalance, &(pool - approved_amount));

        // Clear the farmer's active claim flag so they can file again next season
        let mut farmer_mut = farmer.clone();
        farmer_mut.has_active_claim = false;
        env.storage()
            .persistent()
            .set(&DataKey::Farmer(claim.farmer_id.clone()), &farmer_mut);

        // Transfer USDC from contract to farmer wallet
        let token = soroban_sdk::token::Client::new(&env, &token_contract);
        token.transfer(
            &env.current_contract_address(),
            &farmer.wallet,
            &approved_amount,
        );

        // Emit disbursement event for transparency dashboard
        env.events().publish(
            (Symbol::new(&env, "disburse"), claim_id),
            (farmer.wallet, approved_amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // reject_claim
    // -----------------------------------------------------------------------

    /// Admin rejects a claim (e.g. insufficient evidence, outside disaster zone).
    /// Frees the farmer to file a new claim in the future.
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

        // Re-enable the farmer to file a corrected claim
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
    // get_pool_balance  (read-only)
    // -----------------------------------------------------------------------

    /// Return the current USDC balance in the community relief pool.
    pub fn get_pool_balance(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::PoolBalance)
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // get_claim  (read-only)
    // -----------------------------------------------------------------------

    /// Return a claim record by ID (for front-end status checks).
    pub fn get_claim(env: Env, claim_id: Bytes) -> Result<ClaimRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Claim(claim_id))
            .ok_or(Error::ClaimNotFound)
    }

    // -----------------------------------------------------------------------
    // get_farmer  (read-only)
    // -----------------------------------------------------------------------

    /// Return a farmer record by ID.
    pub fn get_farmer(env: Env, farmer_id: Bytes) -> Result<FarmerRecord, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Farmer(farmer_id))
            .ok_or(Error::FarmerNotFound)
    }
}

// Include test module (compiled only during `cargo test`)
#[cfg(test)]
mod test;