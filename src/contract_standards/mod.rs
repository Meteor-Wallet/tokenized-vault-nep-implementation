use borsh::{BorshDeserialize, BorshSerialize};
use near_contract_standards::fungible_token::{receiver::FungibleTokenReceiver, FungibleTokenCore};
use near_sdk::{
    json_types::U128,
    serde::{Deserialize, Serialize},
    AccountId, PromiseOrValue,
};
use schemars::JsonSchema;
use uint::construct_uint;

pub mod events;

construct_uint! {
    pub struct U256(4);
}

#[derive(
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    JsonSchema,
)]
#[serde(crate = "near_sdk::serde")]
pub enum Asset {
    FungibleToken {
        #[schemars(with = "String")]
        contract_id: AccountId,
    },
    MultiToken {
        #[schemars(with = "String")]
        contract_id: AccountId,
        token_id: String,
    },
}

#[allow(unused)]
pub trait VaultCore: FungibleTokenCore + FungibleTokenReceiver {
    fn asset(&self) -> Asset;
    fn total_assets(&self) -> U128;
    fn redeem(
        &mut self,
        shares: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;
    fn withdraw(
        &mut self,
        assets: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128>;

    fn convert_to_shares(&self, assets: U128) -> U128 {
        if (self.total_assets().0 == 0u128) {
            return assets;
        }

        U256::from(self.ft_total_supply().0)
            .checked_mul(U256::from(assets.0))
            .expect("Too much assets")
            .checked_div(U256::from(self.total_assets().0))
            .unwrap()
            .as_u128()
            .into()
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        assert!(self.ft_total_supply().0 > 0, "No shares issued yet");

        U256::from(shares.0)
            .checked_mul(U256::from(self.total_assets().0))
            .expect("Too many shares")
            .checked_div(U256::from(self.ft_total_supply().0))
            .unwrap()
            .as_u128()
            .into()
    }

    fn max_deposit(&self, receiver_id: AccountId) -> U128 {
        let max_assets = u128::MAX - self.total_assets().0;
        let max_assets_from_shares = self
            .convert_to_assets(U128(u128::MAX - self.ft_total_supply().0))
            .0;

        if max_assets < max_assets_from_shares {
            max_assets.into()
        } else {
            max_assets_from_shares.into()
        }
    }

    fn preview_deposit(&self, assets: U128) -> U128 {
        assert!(assets <= self.max_deposit(near_sdk::env::predecessor_account_id()));
        self.convert_to_shares(assets)
    }

    fn max_mint(&self, receiver_id: AccountId) -> U128 {
        let max_shares = u128::MAX - self.ft_total_supply().0;
        let max_shares_from_assets = self
            .convert_to_shares(U128(u128::MAX - self.total_assets().0))
            .0;

        if max_shares < max_shares_from_assets {
            max_shares.into()
        } else {
            max_shares_from_assets.into()
        }
    }

    fn preview_mint(&self, shares: U128) -> U128 {
        assert!(shares <= self.max_mint(near_sdk::env::predecessor_account_id()));
        self.convert_to_assets(shares)
    }

    fn max_redeem(&self, owner_id: AccountId) -> U128 {
        self.ft_balance_of(owner_id)
    }

    fn preview_redeem(&self, shares: U128) -> U128 {
        assert!(shares <= self.max_redeem(near_sdk::env::predecessor_account_id()));
        self.convert_to_assets(shares)
    }

    fn max_withdraw(&self, owner_id: AccountId) -> U128 {
        self.convert_to_assets(self.ft_balance_of(owner_id))
    }

    fn preview_withdraw(&self, assets: U128) -> U128 {
        assert!(assets <= self.max_withdraw(near_sdk::env::predecessor_account_id()));
        self.convert_to_shares(assets)
    }
}
