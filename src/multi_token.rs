use near_sdk::json_types::U128;
use near_sdk::AccountId;
// use schemars::JsonSchema;
pub trait MultiTokenReceiver {
    /// Handle receiving tokens
    fn mt_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_ids: Vec<String>,
        amounts: Vec<U128>,
        msg: String,
    ) -> Vec<U128>;
}
