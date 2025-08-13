use near_contract_standards::fungible_token::{
    core::ext_ft_core, events::FtBurn, FungibleTokenCore,
};
use near_sdk::{env, ext_contract, json_types::U128, AccountId, Gas, NearToken, Promise};

use crate::{
    mul_div::{mul_div, Rounding},
    TokenizedVault, GAS_FOR_FT_TRANSFER,
};

#[ext_contract(ext_self)]
pub trait _ExtSelf {
    fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    );
}

impl TokenizedVault {
    pub fn internal_transfer_assets_with_callback(
        &self,
        receiver_id: AccountId,
        amount: u128,
        owner: AccountId,
        shares: u128,
        memo: Option<String>,
    ) -> Promise {
        ext_ft_core::ext(self.asset.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(receiver_id.clone(), U128(amount), memo.clone())
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .resolve_withdraw(owner, receiver_id, U128(shares), U128(amount), memo),
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
        self.total_assets = self
            .total_assets
            .checked_sub(assets_to_transfer)
            .expect("total_assets underflow");

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

    pub fn internal_convert_to_shares(&self, assets: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // Handle empty vault case - return 1:1 ratio for first deposit
        if total_supply == 0 {
            return assets;
        }

        let supply_adj = total_supply;
        let assets_adj = self.total_assets + 1;

        mul_div(assets, supply_adj, assets_adj, rounding)
    }

    pub fn internal_convert_to_assets(&self, shares: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        // For empty vault, assume 1:1 ratio for consistency
        if total_supply == 0 {
            return shares;
        }

        let supply_adj = total_supply;
        let assets_adj = self.total_assets + 1;

        mul_div(shares, assets_adj, supply_adj, rounding)
    }
}
