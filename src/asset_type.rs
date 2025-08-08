use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::AccountId;
use near_sdk::serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Asset type enum supporting both FT and MT
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(crate = "near_sdk::serde")]
pub enum AssetType {
    FungibleToken {
        #[schemars(skip)]
        contract_id: AccountId,
    },
    MultiToken {
        #[schemars(skip)]
        contract_id: AccountId,
        token_id: String,
    },
}

impl AssetType {
    pub fn contract_id(&self) -> &AccountId {
        match self {
            AssetType::FungibleToken { contract_id } => contract_id,
            AssetType::MultiToken { contract_id, .. } => contract_id,
        }
    }

    pub fn token_id(&self) -> Option<&String> {
        match self {
            AssetType::FungibleToken { .. } => None,
            AssetType::MultiToken { token_id, .. } => Some(token_id),
        }
    }

    pub fn is_fungible_token(&self) -> bool {
        matches!(self, AssetType::FungibleToken { .. })
    }

    pub fn is_multi_token(&self) -> bool {
        matches!(self, AssetType::MultiToken { .. })
    }
}