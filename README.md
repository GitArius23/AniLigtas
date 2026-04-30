# AniLigtas 🌾

> **On-chain disaster relief funds for Filipino farming cooperatives — no middlemen, no delays.**

---

## One-Line Description

AniLigtas lets farming cooperatives in the Philippines pre-fund a community USDC pool on Stellar, so that when a typhoon or flood hits, affected farmers can file a crop-loss claim and receive an instant on-chain payout — bypassing barangay bureaucracy entirely.

---

## Problem

A rice farmer in Pampanga loses half his crop to Typhoon Carina. To access government or NGO relief funds, he must travel to the municipal hall, submit paper forms, wait 6–10 weeks for manual verification, and hope the cash isn't skimmed before it reaches him. The financial shock of the delay often forces distress selling of land or livestock.

## Solution

AniLigtas deploys a Soroban smart contract that holds a pre-funded USDC community pool contributed by cooperatives, LGUs, and international donors. When disaster strikes, registered farmers file a claim on-chain with an IPFS evidence hash. The cooperative board approves or rejects it in one transaction — and USDC lands directly in the farmer's Stellar wallet within seconds.

---

## Stellar Features Used

| Feature | How It Is Used |
|---|---|
| **Soroban Smart Contracts** | Claim lifecycle (file → approve/reject → disburse), pool accounting, duplicate prevention |
| **USDC / XLM Transfers** | Pool deposits from donors; direct wallet payouts to farmers |
| **Custom Tokens** | Optional cooperative-issued voucher tokens (e.g. `IFUGAO-AID`) for in-kind relief |
| **Trustlines** | Farmer wallets opt in to receive cooperative-issued relief tokens |

---

## Target Users

| Who | Where | Why They Care |
|---|---|---|
| Rice / corn farmers | Central Luzon, Visayas, Mindanao | Get paid immediately after a disaster, not months later |
| Farming cooperative boards | Provincial level, Philippines | Transparent fund management; audit trail for donor reporting |
| NGOs / LGUs / international donors | Philippines & global | Funds go directly to farmers with on-chain proof of disbursement |

---

## Core MVP Transaction Flow

```
[Donor / LGU / NGO]
    │
    ▼
deposit_to_pool(token=USDC, amount=$500)
    │  ← USDC transferred into contract; pool balance updated

[Farmer]
    │
    ▼
file_claim(farmer_id, claim_id, loss=$200, evidence=<IPFS_CID>)
    │  ← Claim stored as Pending; farmer flagged (no double-filing)

[Cooperative Board Admin]
    │
    ▼
approve_and_disburse(claim_id, payout=$150, token=USDC)
    │  ← Claim marked Approved; pool debited; USDC → farmer wallet
    ▼
[Farmer receives $150 USDC in seconds, directly to wallet]
```

Demo time: **< 2 minutes** — deposit → register → file claim → approve & disburse, all on Stellar testnet.

---

## dApp Idea Specification

**PROJECT NAME:** AniLigtas

**PROBLEM:** A rice farmer in Pampanga, Philippines loses his entire harvest to a typhoon and must wait 8 weeks for manual government relief processing, during which he sells livestock at distress prices and takes a predatory informal loan just to feed his family.

**SOLUTION:** AniLigtas deploys a Soroban smart contract holding a cooperative-funded USDC pool; the farmer files a claim on-chain with photo evidence, the cooperative board approves in one transaction, and USDC reaches the farmer's Stellar wallet in seconds — no paper, no travel, no middlemen.

**STELLAR FEATURES USED:**
- Soroban smart contracts (claim registry, fund pool, disbursement logic)
- USDC / XLM transfers (pool deposits, farmer payouts)
- Custom tokens (optional in-kind cooperative vouchers)
- Trustlines (farmer wallet opt-in for cooperative tokens)

**TARGET USERS:**
- Registered cooperative farmers in typhoon-prone Philippine provinces
- Cooperative board admins managing the relief pool
- International NGOs and LGUs providing pool funding

**CORE FEATURE (MVP):** `file_claim()` → `approve_and_disburse()` — a two-transaction flow that takes a verified farmer from filing a crop-loss claim to receiving USDC in their Stellar wallet, with full on-chain event trail.

**WHY THIS WINS:** Disaster relief fraud and delays are a documented crisis in the Philippines; this targets a provably broken system with a demo-able, real-money flow. Judges see USDC moving from an NGO donor to a farmer's wallet in under 30 seconds, backed by tamper-proof on-chain records.

**OPTIONAL EDGE:** AI crop-damage assessment — the IPFS evidence photo is passed to a vision model to generate a recommended payout amount before the admin approves, reducing subjective bias and accelerating decisions.

---

## Suggested MVP Timeline

| Day | Milestone |
|---|---|
| 1 | Scaffold Soroban contract; implement `register_farmer` + `file_claim` |
| 2 | Implement `deposit_to_pool` + `approve_and_disburse` + `reject_claim`; write 3 tests; all pass |
| 3 | Deploy to Stellar testnet; wire up minimal React web UI with Freighter wallet integration |
| 4 | Demo polish: live pool balance, claim status tracker, cooperative dashboard, IPFS evidence upload |

---

## Prerequisites

- **Rust** ≥ 1.74 with `wasm32-unknown-unknown` target
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Soroban CLI** ≥ 20.x
  ```bash
  cargo install --locked soroban-cli --version 20.0.0
  ```
- **Freighter Wallet** browser extension for testnet interaction
- **IPFS / Web3.Storage** account for evidence file uploads (optional for MVP)

---

## Build

```bash
soroban contract build
# Output: target/wasm32-unknown-unknown/release/ani_ligtas.wasm
```

---

## Test

```bash
cargo test
```

Expected output:
```
running 3 tests
test tests::test_happy_path_claim_and_disburse  ... ok
test tests::test_duplicate_active_claim_rejected ... ok
test tests::test_state_after_claim_filing        ... ok

test result: ok. 3 passed; 0 failed
```

---

## Deploy to Testnet

```bash
# 1. Add testnet network
soroban network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"

# 2. Generate admin keypair
soroban keys generate admin --network testnet

# 3. Fund via Friendbot
curl "https://friendbot.stellar.org/?addr=$(soroban keys address admin)"

# 4. Deploy contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/ani_ligtas.wasm \
  --source admin \
  --network testnet
# → CONTRACT_ID returned; save it

# 5. Initialise
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- initialize \
  --admin $(soroban keys address admin)
```

---

## Sample CLI Invocations

### deposit_to_pool

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- deposit_to_pool \
  --token_contract <USDC_CONTRACT_ID> \
  --donor $(soroban keys address admin) \
  --amount 500000000
```

### register_farmer

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- register_farmer \
  --farmer_id "4641524d4552303031" \
  --wallet GFARMERWALLETADDRESSHERE \
  --cooperative_id "4946554741472d434f4f502d3031"
```

### file_claim

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source farmer_key \
  --network testnet \
  -- file_claim \
  --farmer_id "4641524d4552303031" \
  --claim_id "434c41494d2d323032342d303031" \
  --loss_amount 200000000 \
  --evidence_ref "516d5866744d564e426f6f476b796d..."
```

### approve_and_disburse

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- approve_and_disburse \
  --claim_id "434c41494d2d323032342d303031" \
  --approved_amount 150000000 \
  --token_contract <USDC_CONTRACT_ID>
```

### get_pool_balance

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_pool_balance
# → 350000000  (pool now has $350 USDC remaining)
```

---

## Vision & Purpose

The Philippines loses an estimated ₱20–30 billion in agricultural output every typhoon season. Traditional relief disbursement is slow, opaque, and prone to leakage. AniLigtas turns the cooperative's existing trust structure into an on-chain treasury: donors know exactly where funds go, farmers get paid in seconds not months, and cooperative boards have a tamper-proof audit trail to show international funders — making future donations easier to attract.

---

##Deployed Contract Link
https://stellar.expert/explorer/testnet/tx/9c168d76a578e4c90ad3a8dda100c066a91575d4eb9cf5102abbd58ac38043da
https://lab.stellar.org/r/testnet/contract/CCZWFDQGQMWTPTEWKILE6X7AQSDFK5M2SLLZEL46AJKN5DZC4UQSNMHR

## License

MIT License

Copyright (c) 2024 AniLigtas Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.