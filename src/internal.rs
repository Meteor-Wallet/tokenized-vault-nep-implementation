use near_contract_standards::fungible_token::{
    events::{FtBurn, FtMint},
    FungibleTokenCore,
};
use near_sdk::{env, json_types::U128, AccountId, Gas, NearToken, Promise};

use crate::{
    asset_type::AssetType,
    contract_standards::events::VaultDeposit,
    mul_div::{mul_div, Rounding},
    ERC4626Vault, GAS_FOR_FT_TRANSFER,
};

impl ERC4626Vault {
    pub fn internal_transfer_assets_with_callback(
        &self,
        receiver_id: AccountId,
        amount: u128,
        owner: AccountId,
        shares: u128,
        memo: Option<String>,
    ) -> Promise {
        let transfer_promise = match &self.asset {
            AssetType::FungibleToken { contract_id } => {
                Promise::new(contract_id.clone()).function_call(
                    "ft_transfer".to_string(),
                    format!(
                        r#"{{"receiver_id": "{}", "amount": "{}"}}"#,
                        receiver_id, amount
                    )
                    .into_bytes(),
                    NearToken::from_yoctonear(1),
                    GAS_FOR_FT_TRANSFER,
                )
            }
            AssetType::MultiToken {
                contract_id,
                token_id,
            } => {
                Promise::new(contract_id.clone()).function_call(
                    "mt_transfer".to_string(),
                    format!(
                        r#"{{"receiver_id": "{}", "token_id": "{}", "amount": "{}", "approval": null, "memo": null}}"#,
                        receiver_id, token_id, amount
                    )
                    .into_bytes(),
                    NearToken::from_yoctonear(1),
                    GAS_FOR_FT_TRANSFER,
                )
            }
        };

        // Chain with callback to handle success/failure
        transfer_promise.then(
            Promise::new(env::current_account_id()).function_call(
                "resolve_withdraw".to_string(),
                format!(
                    r#"{{"owner": "{}", "receiver": "{}", "shares": "{}", "assets": "{}", "memo": {}}}"#,
                    owner, receiver_id, shares, amount,
                    memo.as_ref().map(|m| format!("\"{}\"", m)).unwrap_or("null".to_string())
                )
                .into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_tgas(10),
            ),
        )
    }

    pub fn internal_execute_withdrawal(
        &mut self,
        owner: AccountId,
        receiver_id: Option<AccountId>,
        shares_to_burn: u128,
        assets_to_transfer: u128,
        memo: Option<String>,
    ) -> Promise {
        let receiver_id = receiver_id.unwrap_or(owner.clone());

        // Checks
        assert!(
            self.token.ft_balance_of(owner.clone()).0 >= shares_to_burn,
            "Insufficient shares"
        );
        assert!(assets_to_transfer > 0, "No assets to withdraw");
        assert!(
            assets_to_transfer <= self.total_assets,
            "Insufficient vault assets"
        );

        // Effects - CEI Pattern: Update state before external call
        // Burn shares immediately (prevents reuse)
        self.token.internal_withdraw(&owner, shares_to_burn);
        self.total_assets -= assets_to_transfer;

        FtBurn {
            owner_id: &owner,
            amount: U128(shares_to_burn),
            memo: Some("Withdrawal"),
        }
        .emit();

        // Interactions - External call
        self.internal_transfer_assets_with_callback(
            receiver_id,
            assets_to_transfer,
            owner,
            shares_to_burn,
            memo,
        )
    }

    pub fn convert_to_shares_internal(&self, assets: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        let supply_adj = total_supply;
        let assets_adj = self.total_assets + 1;

        mul_div(assets, supply_adj, assets_adj, rounding)
    }

    pub fn convert_to_assets_internal(&self, shares: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        if total_supply == 0 {
            return 0; // No assets when no shares exist
        }

        let supply_adj = total_supply;
        let assets_adj = self.total_assets + 1;

        mul_div(shares, assets_adj, supply_adj, rounding)
    }

    // ===== Internal helper for MT deposits =====
    /// Handle MT transfers to the vault
    /// - If msg is "deposit": mint vault shares to sender
    /// - Otherwise: just track assets without minting shares (for donations/yield additions)
    pub fn handle_mt_deposit(
        &mut self,
        sender_id: AccountId,
        token_ids: Vec<String>,
        amounts: Vec<U128>,
        msg: String,
    ) -> Vec<U128> {
        // Only accept if this is an MT asset
        if let AssetType::MultiToken { token_id, .. } = &self.asset {
            assert_eq!(
                env::predecessor_account_id(),
                *self.asset.contract_id(),
                "Only the underlying asset can be deposited"
            );

            // Check that we're receiving the correct token
            assert_eq!(token_ids.len(), 1, "Only single token transfers supported");
            assert_eq!(amounts.len(), 1, "Only single token transfers supported");
            assert_eq!(&token_ids[0], token_id, "Invalid token ID");

            let amount = amounts[0];

            // Deposit: mint shares to sender
            let shares = self.convert_to_shares_internal(amount.0, Rounding::Down);
            self.token.internal_deposit(&sender_id, shares);
            self.total_assets += amount.0;

            FtMint {
                owner_id: &sender_id,
                amount: U128(shares),
                memo: Some("Deposit"),
            }
            .emit();

            // Emit VaultDeposit event
            VaultDeposit {
                sender_id: &sender_id,
                owner_id: &sender_id,
                assets: amount,
                shares: U128(shares),
                memo: None,
            }
            .emit();

            vec![U128(0)] // Accept all tokens
        } else {
            amounts // Reject all tokens if not MT asset
        }
    }
}
