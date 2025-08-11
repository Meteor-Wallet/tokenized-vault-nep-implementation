use crate::helper::{
    mock_ft::{deploy_and_init_mock_ft, ft_balance_of, ft_storage_deposit, ft_transfer},
    vault::{
        deploy_and_init_vault, ft_transfer_call_deposit, vault_asset, vault_balance_of,
        vault_convert_to_assets, vault_convert_to_shares, vault_preview_withdraw, vault_redeem,
        vault_storage_deposit, vault_total_assets, vault_total_supply, vault_withdraw,
    },
};

mod helper;

/// Test basic vault initialization and metadata
#[tokio::test]
async fn test_vault_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Test asset() returns correct underlying asset
    let asset_address = vault_asset(&vault, &owner).await?;
    assert_eq!(asset_address, usdt.id().to_string());

    // Test initial total_assets is 0
    let total_assets = vault_total_assets(&vault, &owner).await?;
    assert_eq!(total_assets.0, 0);

    // Test initial total supply is 0
    let total_supply = vault_total_supply(&vault, &owner).await?;
    assert_eq!(total_supply.0, 0);

    Ok(())
}

/// Test deposit functionality via ft_transfer_call
#[tokio::test]
async fn test_deposit_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Test deposit
    let deposit_amount = 1000u128;
    let result = ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        None,
        None,
    )
    .await?;

    // Verify result is 1000 (used amount) - ft_resolve_transfer returns used amount
    assert_eq!(result.0, 1000);

    // Verify vault state
    let total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(total_assets.0, deposit_amount);

    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_shares.0, deposit_amount); // 1:1 ratio for first deposit

    let total_supply = vault_total_supply(&vault, &alice).await?;
    assert_eq!(total_supply.0, deposit_amount);

    Ok(())
}

/// Test conversion functions (convert_to_shares and convert_to_assets)
#[tokio::test]
async fn test_conversion_functions() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Initial deposit
    let deposit_amount = 1000u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        None,
        None,
    )
    .await?;

    // Test conversion functions with 1:1 ratio (adjusted for inflation resistance)
    let shares_for_500_assets = vault_convert_to_shares(&vault, &alice, 500).await?;
    assert_eq!(shares_for_500_assets.0, 499);

    let assets_for_500_shares = vault_convert_to_assets(&vault, &alice, 500).await?;
    assert_eq!(assets_for_500_shares.0, 500);

    Ok(())
}

/// Test redeem functionality (burn shares for assets)
#[tokio::test]
async fn test_redeem_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Initial deposit
    let deposit_amount = 1000u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        None,
        None,
    )
    .await?;

    let initial_alice_ft_balance = ft_balance_of(&usdt, &alice).await?;
    let initial_alice_shares = vault_balance_of(&vault, &alice, &alice).await?.0;

    // Redeem half the shares
    let redeem_shares = 500u128;
    let assets_received = vault_redeem(&vault, &alice, redeem_shares, None, None).await?;

    // Should receive 500 assets (500 shares at 1:1 ratio)
    assert_eq!(assets_received.0, 500);

    // Verify alice's balances
    let final_alice_ft_balance = ft_balance_of(&usdt, &alice).await?;
    assert_eq!(final_alice_ft_balance, initial_alice_ft_balance + 500);

    let final_alice_shares = vault_balance_of(&vault, &alice, &alice).await?.0;
    assert_eq!(final_alice_shares, initial_alice_shares - 500);

    // Verify vault state
    let final_total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(final_total_assets.0, 500); // 1000 - 500

    let final_total_supply = vault_total_supply(&vault, &alice).await?;
    assert_eq!(final_total_supply.0, 500); // 1000 - 500

    Ok(())
}

/// Test withdraw functionality (burn shares to get specific asset amount)
#[tokio::test]
async fn test_withdraw_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Initial deposit
    let deposit_amount = 1000u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        None,
        None,
    )
    .await?;

    let initial_alice_ft_balance = ft_balance_of(&usdt, &alice).await?;
    let initial_alice_shares = vault_balance_of(&vault, &alice, &alice).await?.0;

    // Withdraw specific asset amount
    let withdraw_assets = 500u128;
    let shares_used = vault_withdraw(&vault, &alice, withdraw_assets, None, None).await?;

    // Should use 500 shares (500 assets at 1:1 ratio, rounded up)
    assert_eq!(shares_used.0, 500);

    // Verify alice's balances
    let final_alice_ft_balance = ft_balance_of(&usdt, &alice).await?;
    assert_eq!(final_alice_ft_balance, initial_alice_ft_balance + 500);

    let final_alice_shares = vault_balance_of(&vault, &alice, &alice).await?.0;
    assert_eq!(final_alice_shares, initial_alice_shares - 500);

    // Verify vault state
    let final_total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(final_total_assets.0, 500); // 1000 - 500

    let final_total_supply = vault_total_supply(&vault, &alice).await?;
    assert_eq!(final_total_supply.0, 500); // 1000 - 500

    Ok(())
}

/// Test preview_withdraw function
#[tokio::test]
async fn test_preview_withdraw() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Initial deposit
    let deposit_amount = 1000u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        None,
        None,
    )
    .await?;

    // Test preview_withdraw
    let preview_shares = vault_preview_withdraw(&vault, &alice, 500).await?;
    // 500 * 1000 / 1001 = 499.5, rounded up = 500 shares
    assert_eq!(preview_shares.0, 500);

    // Verify actual withdraw matches preview
    let actual_shares_used = vault_withdraw(&vault, &alice, 500, None, None).await?;
    assert_eq!(actual_shares_used.0, preview_shares.0);

    Ok(())
}

/// Test deposit with receiver_id parameter
#[tokio::test]
async fn test_deposit_with_receiver() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    vault_storage_deposit(&vault, &bob).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Alice deposits but shares go to Bob
    let deposit_amount = 1000u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        Some(&bob),
        None,
        None,
        None,
    )
    .await?;

    // Verify alice has no shares
    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_shares.0, 0);

    // Verify bob received the shares
    let bob_shares = vault_balance_of(&vault, &alice, &bob).await?;
    assert_eq!(bob_shares.0, deposit_amount);

    Ok(())
}

/// Test deposit with min_shares and max_shares parameters  
#[tokio::test]
async fn test_deposit_with_slippage_protection() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Test with min_shares that should pass
    let deposit_amount = 1000u128;
    let min_shares = 900u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        Some(min_shares),
        None,
        None,
    )
    .await?;

    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_shares.0, deposit_amount);

    Ok(())
}

/// Test deposit with max_shares that should refund excess
#[tokio::test]
async fn test_deposit_max_shares_with_refund() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Alice tries to deposit 1000 USDT but sets max_shares to 500 (should only mint 500 shares, refund the rest)
    let deposit_amount = 1000u128;
    let max_shares = 500u128;
    let result = ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,
        None,
        Some(max_shares),
        None,
    )
    .await?;

    // Only 500 shares minted, so only 500 USDT used, 500 refunded
    assert_eq!(result.0, 500);

    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_shares.0, 500);

    let alice_ft_balance = ft_balance_of(&usdt, &alice).await?;
    // She started with 10000, deposited 1000, but 500 refunded, so should have 9500
    assert_eq!(alice_ft_balance, 9500);

    let total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(total_assets.0, 500);

    let total_supply = vault_total_supply(&vault, &alice).await?;
    assert_eq!(total_supply.0, 500);

    Ok(())
}

/// Test multiple users with same conversion rates
#[tokio::test]
async fn test_multi_user_same_rates() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    ft_storage_deposit(&usdt, &bob).await?;
    vault_storage_deposit(&vault, &alice).await?;
    vault_storage_deposit(&vault, &bob).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;
    ft_transfer(&usdt, &owner, &bob, 10000).await?;

    // Alice deposits first (1:1 ratio)
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    // Bob deposits same amount at same rate
    ft_transfer_call_deposit(&usdt, &vault, &bob, 1000, None, None, None, None).await?;

    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    let bob_shares = vault_balance_of(&vault, &alice, &bob).await?;

    assert_eq!(alice_shares.0, 1000);
    assert_eq!(bob_shares.0, 999); // Due to inflation resistance adjustment

    // Total assets should be 2000
    let total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(total_assets.0, 2000);

    // Total supply should be 1999
    let total_supply = vault_total_supply(&vault, &alice).await?;
    assert_eq!(total_supply.0, 1999);

    Ok(())
}
