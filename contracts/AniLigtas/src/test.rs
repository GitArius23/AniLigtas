#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Events},
        token::{Client as TokenClient, StellarAssetClient},
        Address, Bytes, Env, IntoVal,
    };

    use crate::{AniLigtasContract, AniLigtasContractClient, Error};

    // -----------------------------------------------------------------------
    // Helper: build a 32-byte Bytes value from a seed byte
    // -----------------------------------------------------------------------
    fn make_bytes(env: &Env, seed: u8) -> Bytes {
        let mut raw = [0u8; 32];
        raw[0] = seed;
        Bytes::from_array(env, &raw)
    }

    // -----------------------------------------------------------------------
    // Helper: deploy contract + USDC mock token; fund the pool; return handles
    // -----------------------------------------------------------------------
    fn setup() -> (
        Env,
        AniLigtasContractClient<'static>,
        Address, // token contract (mock USDC)
        Address, // admin
        Address, // farmer wallet
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let farmer_wallet = Address::generate(&env);

        // Deploy the AniLigtas contract
        let contract_id = env.register_contract(None, AniLigtasContract);
        let client = AniLigtasContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        // Create a mock Stellar asset to represent USDC
        let token_id = env.register_stellar_asset_contract(admin.clone());
        let asset_admin = StellarAssetClient::new(&env, &token_id);

        // Mint USDC to both the admin (for pool seeding) and the contract itself
        asset_admin.mint(&admin, &1_000_000_000_i128); // $10,000 USDC (6 decimals)

        (env, client, token_id, admin, farmer_wallet)
    }

    // -----------------------------------------------------------------------
    // Test 1 — Happy path
    // A farmer files a claim, the admin approves it, and USDC lands in the
    // farmer's wallet from the community pool.
    // -----------------------------------------------------------------------
    #[test]
    fn test_happy_path_claim_and_disburse() {
        let (env, client, token_id, admin, farmer_wallet) = setup();

        let farmer_id = make_bytes(&env, 1);
        let coop_id   = make_bytes(&env, 10);
        let claim_id  = make_bytes(&env, 20);
        let evidence  = make_bytes(&env, 30);

        // Fund the community pool with $500 USDC
        let pool_deposit: i128 = 500_000_000;
        client.deposit_to_pool(&token_id, &admin, &pool_deposit);
        assert_eq!(client.get_pool_balance(), pool_deposit);

        // Register the farmer
        client.register_farmer(&farmer_id, &farmer_wallet, &coop_id);

        // Farmer files a crop-loss claim for $200 USDC
        let loss: i128 = 200_000_000;
        client.file_claim(&farmer_id, &claim_id, &loss, &evidence);

        // Admin approves and disburses $150 USDC (partial, reflecting assessed damage)
        let payout: i128 = 150_000_000;
        client.approve_and_disburse(&claim_id, &payout, &token_id);

        // Farmer wallet must now hold the payout
        let token = TokenClient::new(&env, &token_id);
        assert_eq!(token.balance(&farmer_wallet), payout);

        // Pool balance must have decreased by the payout
        assert_eq!(client.get_pool_balance(), pool_deposit - payout);

        // Claim status must be Approved (1)
        let claim = client.get_claim(&claim_id);
        assert_eq!(claim.status, 1);
        assert_eq!(claim.approved_amount, payout);
    }

    // -----------------------------------------------------------------------
    // Test 2 — Edge case
    // A farmer cannot file two claims simultaneously; the second attempt must
    // return Error::ActiveClaimExists.
    // -----------------------------------------------------------------------
    #[test]
    fn test_duplicate_active_claim_rejected() {
        let (env, client, token_id, admin, farmer_wallet) = setup();

        let farmer_id  = make_bytes(&env, 2);
        let coop_id    = make_bytes(&env, 11);
        let claim_id_a = make_bytes(&env, 21);
        let claim_id_b = make_bytes(&env, 22);
        let evidence   = make_bytes(&env, 31);

        client.deposit_to_pool(&token_id, &admin, &500_000_000_i128);
        client.register_farmer(&farmer_id, &farmer_wallet, &coop_id);

        // First claim succeeds
        client.file_claim(&farmer_id, &claim_id_a, &100_000_000_i128, &evidence);

        // Second claim while first is still pending must fail
        let result = client.try_file_claim(
            &farmer_id,
            &claim_id_b,
            &80_000_000_i128,
            &evidence,
        );
        assert!(result.is_err());
        let sdk_err = result.unwrap_err().unwrap();
        assert_eq!(sdk_err, Error::ActiveClaimExists.into_val(&env));
    }

    // -----------------------------------------------------------------------
    // Test 3 — State verification
    // After a successful claim filing, contract storage must correctly reflect:
    //   • The claim record with status Pending (0) and correct loss amount
    //   • The farmer record with has_active_claim = true
    //   • At least one on-chain event was emitted
    // -----------------------------------------------------------------------
    #[test]
    fn test_state_after_claim_filing() {
        let (env, client, token_id, admin, farmer_wallet) = setup();

        let farmer_id = make_bytes(&env, 3);
        let coop_id   = make_bytes(&env, 12);
        let claim_id  = make_bytes(&env, 23);
        let evidence  = make_bytes(&env, 32);
        let loss: i128 = 75_000_000; // $75 USDC

        client.deposit_to_pool(&token_id, &admin, &500_000_000_i128);
        client.register_farmer(&farmer_id, &farmer_wallet, &coop_id);
        client.file_claim(&farmer_id, &claim_id, &loss, &evidence);

        // Claim record must show correct loss and Pending status
        let claim = client.get_claim(&claim_id);
        assert_eq!(claim.status, 0);          // Pending
        assert_eq!(claim.loss_amount, loss);
        assert_eq!(claim.approved_amount, 0); // Not yet approved

        // Farmer record must flag an active claim
        let farmer = client.get_farmer(&farmer_id);
        assert!(farmer.has_active_claim);
        assert_eq!(farmer.wallet, farmer_wallet);

        // At least the enrollment, deposit, and claim events should exist
        let events = env.events().all();
        assert!(events.len() >= 3);
    }
}