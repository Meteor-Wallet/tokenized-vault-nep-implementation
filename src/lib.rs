pub mod asset_type;
mod mul_div;
mod multi_token;
mod contract_standards;

use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    core_impl::FungibleToken,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC},
    receiver::FungibleTokenReceiver,
    FungibleTokenResolver,
};
use near_contract_standards::storage_management::StorageManagement;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near_bindgen, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseOrValue,
};

use crate::asset_type::AssetType;
use crate::contract_standards::FungibleTokenVaultCore;
use crate::mul_div::{mul_div, Rounding};
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

#[near_bindgen]
impl ERC4626Vault {
    #[init]
    pub fn new(
        asset: AssetType,
        name: String,
        symbol: String,
        decimals: u8,
        icon: Option<String>,
    ) -> Self {
        Self {
            token: FungibleToken::new(b"s"),
            metadata: FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name,
                symbol,
                icon,
                reference: None,
                reference_hash: None,
                decimals,
            },
            asset,
            total_assets: 0,
            owner: env::predecessor_account_id(),
        }
    }

    // ===== View Functions =====
    pub fn asset_type(&self) -> AssetType {
        self.asset.clone()
    }

    // ===== Internal Helpers =====

    fn internal_transfer_assets(&self, receiver_id: AccountId, amount: u128) -> Promise {
        match &self.asset {
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
            },
            AssetType::MultiToken { contract_id, token_id } => {
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
        }
    }

    fn convert_to_shares_internal(&self, assets: u128, rounding: Rounding) -> u128 {
        let total_supply = self.token.ft_total_supply().0;

        let supply_adj = total_supply;
        let assets_adj = self.total_assets + 1;

        mul_div(assets, supply_adj, assets_adj, rounding)
    }

    fn convert_to_assets_internal(&self, shares: u128, rounding: Rounding) -> u128 {
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
    fn handle_mt_deposit(
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
            
            // Check message to determine action
            if msg == "deposit" {
                // Deposit: mint shares to sender
                let shares = self.convert_to_shares_internal(amount.0, Rounding::Down);
                self.token.internal_deposit(&sender_id, shares);
                self.total_assets += amount.0;
            } else {
                // Just track assets without minting shares
                self.total_assets += amount.0;
            }

            vec![U128(0)] // Accept all tokens
        } else {
            amounts // Reject all tokens if not MT asset
        }
    }
}

// ===== Implement FungibleTokenVaultCore Trait =====
#[near_bindgen]
impl FungibleTokenVaultCore for ERC4626Vault {
    fn asset(&self) -> AccountId {
        self.asset.contract_id().clone()
    }

    fn total_assets(&self) -> U128 {
        U128(self.total_assets)
    }

    fn redeem(&mut self, shares: U128, receiver_id: Option<AccountId>) -> PromiseOrValue<U128> {
        let owner = env::predecessor_account_id();
        let receiver_id = receiver_id.unwrap_or(owner.clone());
        
        let assets = self.convert_to_assets_internal(shares.0, Rounding::Down);

        // Burn shares
        self.token.internal_withdraw(&owner, shares.0);
        self.total_assets -= assets;

        // Transfer underlying assets and return promise
        PromiseOrValue::Promise(
            self.internal_transfer_assets(receiver_id, assets)
        )
    }

    fn withdraw(&mut self, assets: U128, receiver_id: Option<AccountId>) -> PromiseOrValue<U128> {
        let owner = env::predecessor_account_id();
        let receiver_id = receiver_id.unwrap_or(owner.clone());


        let shares = self.convert_to_shares_internal(assets.0, Rounding::Up);

        // Burn shares
        self.token.internal_withdraw(&owner, shares);
        self.total_assets -= assets.0;

        // Transfer underlying assets
        PromiseOrValue::Promise(
            self.internal_transfer_assets(receiver_id, assets.0)
        )
    }

    fn convert_to_shares(&self, assets: U128) -> U128 {
        U128(self.convert_to_shares_internal(assets.0, Rounding::Down))
    }

    fn convert_to_assets(&self, shares: U128) -> U128 {
        U128(self.convert_to_assets_internal(shares.0, Rounding::Down))
    }

    fn max_deposit(&self, receiver_id: AccountId) -> U128 {
        U128(u128::MAX - self.total_assets)
    }

    fn preview_deposit(&self, assets: U128) -> U128 {
        self.convert_to_shares(assets)
    }

    fn max_redeem(&self, owner_id: AccountId) -> U128 {
        self.token.ft_balance_of(owner_id)
    }

    fn preview_redeem(&self, shares: U128) -> U128 {
        self.convert_to_assets(shares)
    }

    fn max_withdraw(&self, owner: AccountId) -> U128 {
       U128(self.convert_to_shares_internal(self.token.ft_balance_of(owner).0, Rounding::Down))
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
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
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
