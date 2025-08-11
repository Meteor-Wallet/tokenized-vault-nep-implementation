use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{Account, Contract};
use serde_json::json;

pub async fn deploy_and_init_mock_ft(
    owner: &Account,
    total_supply: Option<u128>,
) -> Result<Contract, Box<dyn std::error::Error>> {
    let contract_code = near_workspaces::compile_project("./mock_contracts/mock_ft").await?;

    let contract = owner.deploy(&contract_code).await?.into_result()?;

    contract
        .call("new_default_meta")
        .args_json(json!({
            "owner_id": owner.id(),
            "total_supply": total_supply.unwrap_or(u128::MAX).to_string(),
        }))
        .transact()
        .await?
        .into_result()?;

    Ok(contract)
}

pub async fn ft_storage_deposit(
    contract: &Contract,
    account: &Account,
) -> Result<(), Box<dyn std::error::Error>> {
    account
        .call(contract.id(), "storage_deposit")
        .args_json(json!({
            "account_id": account.id(),
            "registration_only": false,
        }))
        .deposit(NearToken::from_near(1))
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

pub async fn ft_transfer(
    contract: &Contract,
    sender: &Account,
    receiver: &Account,
    amount: u128,
) -> Result<(), Box<dyn std::error::Error>> {
    sender
        .call(contract.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": receiver.id(),
            "amount": amount.to_string(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

pub async fn ft_balance_of(
    contract: &Contract,
    account: &Account,
) -> Result<u128, Box<dyn std::error::Error>> {
    let result: U128 = account
        .view(contract.id(), "ft_balance_of")
        .args_json(json!({
            "account_id": account.id(),
        }))
        .await?
        .json()?;

    Ok(result.0)
}
