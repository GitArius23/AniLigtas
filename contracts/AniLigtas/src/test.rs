#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Events},
        token::{TokenClient, StellarAssetClient},
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
    // Helper: deploy contract + mock USDC token, seed the pool, return handles
    // -----------------------------------------------------------------------
    fn setup() -> (
        Env,
        AniLigtasContractClient<'static>,
        Address, // mock token contract (USDC)
        Address, // admin
        Address, // farmer wallet
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let farmer_wallet = Address::generate(&env);

        // Deploy AniLigtas contract
        let contract_id = env.register_contract(None, AniLigtasContract);
        let client = AniLigtasContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        // register_stellar_asset_contract_v2 is the correct call in SDK v20/v21
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let asset_admin = StellarAssetClient::new(&env, &token_id.address());

        // Mint $10,000 USDC (7 decimals, but we treat as 6 here for simplicity)
        asset_admin.mint(&admin, &1_000_000_000_i128);

        (env, client, token_id.address(), admin, farmer_wallet)
    }

    // -----------------------------------------------------------------------
    // Test 1 — Happy path
    // Farmer files claim → admin approves → USDC lands in farmer wallet
    // -----------------------------------------------------------------------
    #[test]
    fn test_happy_path_claim_and_disburse() {
        let (env, client, token_id, admin, farmer_wallet) = setup();

        let farmer_id = make_bytes(&env, 1);
        let coop_id   = make_bytes(&env, 10);
        let claim_id  = make_bytes(&env, 20);
        let evidence  = make_bytes(&env, 30);

        // Seed pool with $500 USDC
        let pool_deposit: i128 = 500_000_000;
        client.deposit_to_pool(&token_id, &admin, &pool_deposit);
        assert_eq!(client.get_pool_balance(), pool_deposit);

        // Register farmer
        client.register_farmer(&farmer_id, &farmer_wallet, &coop_id);

        // Farmer files a $200 loss claim
        client.file_claim(&farmer_id, &claim_id, &200_000_000_i128, &evidence);

        // Admin approves $150 payout
        let payout: i128 = 150_000_000;
        client.approve_and_disburse(&claim_id, &payout, &token_id);

        // Farmer wallet must hold exactly the payout amount
        let token = TokenClient::new(&env, &token_id);
        assert_eq!(token.balance(&farmer_wallet), payout);

        // Pool must be debited correctly
        assert_eq!(client.get_pool_balance(), pool_deposit - payout);

        // Claim must be marked Approved (status = 1)
        let claim = client.get_claim(&claim_id);
        assert_eq!(claim.status, 1);
        assert_eq!(claim.approved_amount, payout);
    }

    // -----------------------------------------------------------------------
    // Test 2 — Edge case
    // A farmer cannot file two simultaneous claims; second attempt must return
    // Error::ActiveClaimExists
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

        // Second claim while first is pending must fail
        let result = client.try_file_claim(
            &farmer_id,
            &claim_id_b,
            &80_000_000_i128,
            &evidence,
        );
        assert!(result.is_err());

        // Confirm it's specifically the ActiveClaimExists error
        let sdk_err = result.unwrap_err().unwrap();
        assert_eq!(sdk_err, Error::ActiveClaimExists.into_val(&env));
    }

    // -----------------------------------------------------------------------
    // Test 3 — State verification
    // After filing a claim, contract storage must show:
    //   • ClaimRecord with status=0 (Pending) and correct loss_amount
    //   • FarmerRecord with has_active_claim = true
    //   • At least 3 on-chain events emitted (enroll, deposit, claim)
    // -----------------------------------------------------------------------
    #[test]
    fn test_state_after_claim_filing() {
        let (env, client, token_id, admin, farmer_wallet) = setup();

        let farmer_id = make_bytes(&env, 3);
        let coop_id   = make_bytes(&env, 12);
        let claim_id  = make_bytes(&env, 23);
        let evidence  = make_bytes(&env, 32);
        let loss: i128 = 75_000_000;

        client.deposit_to_pool(&token_id, &admin, &500_000_000_i128);
        client.register_farmer(&farmer_id, &farmer_wallet, &coop_id);
        client.file_claim(&farmer_id, &claim_id, &loss, &evidence);

        // Claim must be Pending with the correct loss amount
        let claim = client.get_claim(&claim_id);
        assert_eq!(claim.status, 0);
        assert_eq!(claim.loss_amount, loss);
        assert_eq!(claim.approved_amount, 0);

        // Farmer must have active claim flagged
        let farmer = client.get_farmer(&farmer_id);
        assert!(farmer.has_active_claim);
        assert_eq!(farmer.wallet, farmer_wallet);

        // At least enroll + deposit + claim events must exist
        let events = env.events().all();
        assert!(events.len() >= 3);
    }
}