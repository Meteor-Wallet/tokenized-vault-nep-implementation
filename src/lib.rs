mod contract_standards;
mod internal;
mod mul_div;

use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    core_impl::FungibleToken,
    events::FtMint,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider},
    receiver::FungibleTokenReceiver,
    FungibleTokenResolver,
};
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::{
    assert_one_yocto,
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::Deserialize,
};
use near_sdk::{env, near_bindgen, AccountId, Gas, NearToken, PanicOnDefault, PromiseOrValue};
use near_sdk::{json_types::U128, BorshStorageKey};

use crate::contract_standards::events::{VaultDeposit, VaultWithdraw};
use crate::contract_standards::VaultCore;
use crate::mul_div::Rounding;

const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(30);

#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DepositMessage {
    min_shares: Option<U128>,
    max_shares: Option<U128>,
    receiver_id: Option<AccountId>,
    memo: Option<String>,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct ERC4626Vault {
    pub token: FungibleToken,        // Vault shares (NEP-141)
    metadata: FungibleTokenMetadata, // Metadata for shares
    asset: AccountId,                // Underlying asset (NEP-141 or NEP-245)
    total_assets: u128,              // Total managed assets
    owner: AccountId,                // Vault owner
}

#[derive(BorshSerialize, BorshDeserialize, BorshStorageKey)]
pub enum StorageKey {
    FungibleToken,
}

#[near_bindgen]
impl ERC4626Vault {
    #[init]
    pub fn new(asset: AccountId, metadata: FungibleTokenMetadata) -> Self {
        Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata,
            asset,
            total_assets: 0,
            owner: env::predecessor_account_id(),
        }
    }

    #[private]
    pub fn resolve_withdraw(
        &mut self,
        owner: AccountId,
        receiver: AccountId,
        shares: U128,
        assets: U128,
        memo: Option<String>,
    ) -> U128 {
        // Check if the transfer succeeded
        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                // Transfer succeeded - finalize withdrawal

                // Emit VaultWithdraw event
                VaultWithdraw {
                    owner_id: &owner,
                    receiver_id: &receiver,
                    assets,
                    shares,
                    memo: memo.as_deref(),
                }
                .emit();

                assets
            }
            _ => {
                // Transfer failed - rollback state changes using callback parameters
                // Restore shares that were burned
                self.token.internal_deposit(&owner, shares.0);
                // Restore total_assets that was reduced
                self.total_assets += assets.0;

                FtMint {
                    owner_id: &owner,
                    amount: U128(shares.0),
                    memo: Some("Withdrawal rollback"),
                }
                .emit();

                0.into()
            }
        }
    }
}

// ===== Implement FungibleTokenVaultCore Trait =====
#[near_bindgen]
impl VaultCore for ERC4626Vault {
    fn asset(&self) -> AccountId {
        self.asset.clone()
    }

    fn total_assets(&self) -> U128 {
        U128(self.total_assets)
    }

    #[payable]
    fn redeem(
        &mut self,
        shares: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();

        let owner = env::predecessor_account_id();

        assert!(
            shares.0 <= self.max_redeem(owner.clone()).0,
            "Exceeds max redeem"
        );

        let assets = self.internal_convert_to_assets(shares.0, Rounding::Down);

        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            receiver_id,
            shares.0,
            assets,
            memo,
        ))
    }

    #[payable]
    fn withdraw(
        &mut self,
        assets: U128,
        receiver_id: Option<AccountId>,
        memo: Option<String>,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();

        let owner = env::predecessor_account_id();
        assert!(
            assets.0 <= self.max_withdraw(owner.clone()).0,
            "Exceeds max withdraw"
        );

        let shares = self.internal_convert_to_shares(assets.0, Rounding::Up);

        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            receiver_id,
            shares,
            assets.0,
            memo,
        ))
    }

    fn convert_to_shares(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares(assets.0, Rounding::Down))
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.internal_convert_to_assets(shares.0, Rounding::Down))
    }

    fn preview_withdraw(&self, assets: U128) -> U128 {
        U128(self.internal_convert_to_shares(assets.0, Rounding::Up))
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for ERC4626Vault {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(
            env::predecessor_account_id(),
            self.asset.clone(),
            "Only the underlying asset can be deposited"
        );

        let parsed_msg = match serde_json::from_str::<DepositMessage>(&msg) {
            Ok(deposit_message) => deposit_message,
            Err(_) => DepositMessage {
                min_shares: None,
                max_shares: None,
                receiver_id: None,
                memo: None,
            },
        };

        let max_new_shares = self.convert_to_shares(amount).0;

        if let Some(min_shares) = parsed_msg.min_shares {
            assert!(
                max_new_shares >= min_shares.0,
                "Slippage error, insufficient shares minted: {} < {}",
                max_new_shares,
                min_shares.0
            );
        }

        let shares = if let Some(max_shares) = parsed_msg.max_shares {
            if max_new_shares > max_shares.0 {
                max_shares.0
            } else {
                max_new_shares
            }
        } else {
            max_new_shares
        };

        let used_amount = self.internal_convert_to_assets(shares, Rounding::Up);
        let unused_amount = amount
            .0
            .checked_sub(used_amount)
            .expect("Overflow in unused amount calculation");

        assert!(
            used_amount > 0,
            "No assets to deposit, shares: {}, amount: {}",
            shares,
            amount.0
        );

        self.token.internal_deposit(&sender_id, shares);
        self.total_assets += used_amount;

        let owner_id = parsed_msg.receiver_id.unwrap_or(sender_id.clone());

        FtMint {
            owner_id: &owner_id,
            amount: U128(shares),
            memo: Some("Deposit"),
        }
        .emit();

        // Emit VaultDeposit event
        VaultDeposit {
            sender_id: &sender_id,
            owner_id: &owner_id,
            assets: amount,
            shares: U128(shares),
            memo: parsed_msg.memo.as_deref(),
        }
        .emit();

        PromiseOrValue::Value(U128(unused_amount))
    }
}

// ===== Implement Fungible Token Traits for Vault Shares =====
#[near_bindgen]
impl FungibleTokenCore for ERC4626Vault {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near_bindgen]
impl FungibleTokenResolver for ERC4626Vault {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        self.token
            .ft_resolve_transfer(sender_id, receiver_id, amount)
    }
}

#[near_bindgen]
impl StorageManagement for ERC4626Vault {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(
        &mut self,
        amount: Option<NearToken>,
    ) -> near_contract_standards::storage_management::StorageBalance {
        self.token.storage_withdraw(amount)
    }

    fn storage_balance_bounds(
        &self,
    ) -> near_contract_standards::storage_management::StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(
        &self,
        account_id: AccountId,
    ) -> Option<near_contract_standards::storage_management::StorageBalance> {
        self.token.storage_balance_of(account_id)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.token.storage_unregister(force)
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for ERC4626Vault {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}
