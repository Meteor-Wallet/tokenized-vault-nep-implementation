pub mod asset_type;
mod contract_standards;
mod internal;
mod mul_div;
mod multi_token;

use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    core_impl::FungibleToken,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC},
    receiver::FungibleTokenReceiver,
    FungibleTokenResolver,
};
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::{
    assert_one_yocto,
    borsh::{self, BorshDeserialize, BorshSerialize},
};
use near_sdk::{env, near_bindgen, AccountId, Gas, NearToken, PanicOnDefault, PromiseOrValue};
use near_sdk::{json_types::U128, BorshStorageKey};

use crate::asset_type::AssetType;
use crate::contract_standards::events::{VaultDeposit, VaultWithdraw};
use crate::contract_standards::FungibleTokenVaultCore;
use crate::mul_div::Rounding;
use crate::multi_token::MultiTokenReceiver;

const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(30);

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct ERC4626Vault {
    pub token: FungibleToken,        // Vault shares (NEP-141)
    metadata: FungibleTokenMetadata, // Metadata for shares
    asset: AssetType,                // Underlying asset (NEP-141 or NEP-245)
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
    pub fn new(asset: AssetType, metadata: FungibleTokenMetadata) -> Self {
        Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata,
            asset,
            total_assets: 0,
            owner: env::predecessor_account_id(),
        }
    }

    // TODO: Either the NEP spec needed to be changed, or the asset type needed to be changed
    // A string can not represent the underlying asset if the asset is NEP-245
    // Further edit after Edward makes decision
    pub fn asset_type(&self) -> AssetType {
        self.asset.clone()
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

                env::panic_str("Asset transfer failed - state rolled back")
            }
        }
    }
}

// ===== Implement FungibleTokenVaultCore Trait =====
#[near_bindgen]
impl FungibleTokenVaultCore for ERC4626Vault {
    // TODO: Either the NEP spec needed to be changed, or the asset type needed to be changed
    // A string can not represent the underlying asset if the asset is NEP-245
    // Further edit after Edward makes decision
    fn asset(&self) -> AccountId {
        self.asset.contract_id().clone()
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
        let assets = self.convert_to_assets_internal(shares.0, Rounding::Down);

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
        let shares = self.convert_to_shares_internal(assets.0, Rounding::Up);

        PromiseOrValue::Promise(self.internal_execute_withdrawal(
            owner,
            receiver_id,
            shares,
            assets.0,
            memo,
        ))
    }

    fn convert_to_shares(&self, assets: U128) -> U128 {
        U128(self.convert_to_shares_internal(assets.0, Rounding::Down))
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.convert_to_assets_internal(shares.0, Rounding::Down))
    }

    fn preview_withdraw(&self, assets: U128) -> U128 {
        U128(self.convert_to_shares_internal(assets.0, Rounding::Up))
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for ERC4626Vault {
    /// Handle FT transfers to the vault
    /// - If msg is "deposit": mint vault shares to sender
    /// - Otherwise: just track assets without minting shares (for donations/yield additions)
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Only accept if this is an FT asset
        if let AssetType::FungibleToken { .. } = &self.asset {
            assert_eq!(
                env::predecessor_account_id(),
                *self.asset.contract_id(),
                "Only the underlying asset can be deposited"
            );

            // Check message to determine action
            if msg == "deposit" {
                // Deposit: mint shares to sender
                let shares = self.convert_to_shares(amount).0;
                self.token.internal_deposit(&sender_id, shares);
                self.total_assets += amount.0;

                // Emit VaultDeposit event
                VaultDeposit {
                    sender_id: &sender_id,
                    owner_id: &sender_id,
                    assets: amount,
                    shares: U128(shares),
                    memo: None,
                }
                .emit();
            } else {
                // Just track assets without minting shares
                self.total_assets += amount.0;
            }

            PromiseOrValue::Value(U128(0)) // Accept all tokens
        } else {
            PromiseOrValue::Value(amount) // Reject all tokens if not FT asset
        }
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
impl MultiTokenReceiver for ERC4626Vault {
    fn mt_on_transfer(
        &mut self,
        sender_id: AccountId,
        _previous_owner_id: AccountId,
        token_ids: Vec<String>,
        amounts: Vec<U128>,
        msg: String,
    ) -> Vec<U128> {
        self.handle_mt_deposit(sender_id, token_ids, amounts, msg)
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
