use near_contract_standards::fungible_token::{receiver::FungibleTokenReceiver, FungibleTokenCore};
use near_sdk::{json_types::U128, AccountId, PromiseOrValue};
#[allow(unused)]
pub trait FungibleTokenVaultCore: FungibleTokenCore + FungibleTokenReceiver {
    fn asset(&self) -> AccountId;
    fn total_assets(&self) -> U128;
    fn redeem(&mut self, shares: U128, receiver: Option<AccountId>) -> PromiseOrValue<U128>;

    fn convert_to_shares(&self, assets: U128) -> U128 {
        if (self.total_assets().0 == 0u128) {
            return assets;
        }

        // TODO: upscale u128 to become u256 when multiplying/dividing, then downscale to u128
        // to avoid overflow. Perform checks to ensure no overflow occurs.
        self.ft_total_supply()
            .0
            .checked_mul(assets.0)
            .expect("Too much assets")
            .checked_div(self.total_assets().0)
            .unwrap()
            .into()
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        assert!(self.ft_total_supply().0 > 0, "No shares issued yet");

        // TODO: upscale u128 to become u256 when multiplying/dividing, then downscale to u128
        // to avoid overflow. Perform checks to ensure no overflow occurs.
        shares
            .0
            .checked_mul(self.total_assets().0)
            .expect("Too many shares")
            .checked_div(self.ft_total_supply().0)
            .unwrap()
            .into()
    }

    fn max_deposit(&self, receiver: AccountId) -> U128 {
        (u128::MAX - self.total_assets().0).into()
    }

    fn preview_deposit(&self, assets: U128) -> U128 {
        assert!(assets <= self.max_deposit(near_sdk::env::predecessor_account_id()));
        self.convert_to_shares(assets)
    }

    fn max_redeem(&self, owner: AccountId) -> U128 {
        self.ft_balance_of(owner)
    }

    fn preview_redeem(&self, shares: U128) -> U128 {
        assert!(shares <= self.max_redeem(near_sdk::env::predecessor_account_id()));
        self.convert_to_assets(shares)
    }
}
