use crate::helper::{
    mock_ft::{deploy_and_init_mock_ft, ft_balance_of, ft_storage_deposit, ft_transfer},
    vault::{
        deploy_and_init_vault, ft_transfer_call_deposit, vault_balance_of, vault_convert_to_assets,
        vault_convert_to_shares, vault_redeem, vault_storage_deposit, vault_total_assets,
        vault_total_supply, vault_withdraw,
    },
};

mod helper;

/// Test empty vault edge cases
#[tokio::test]
async fn test_empty_vault_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Test conversions on empty vault
    let shares_for_zero = vault_convert_to_shares(&vault, &owner, 0).await?;
    assert_eq!(shares_for_zero.0, 0);

    let shares_for_1000 = vault_convert_to_shares(&vault, &owner, 1000).await?;
    assert_eq!(shares_for_1000.0, 1000); // 1:1 ratio when empty

    let assets_for_zero = vault_convert_to_assets(&vault, &owner, 0).await?;
    assert_eq!(assets_for_zero.0, 0);

    Ok(())
}

/// Test rounding behavior to prevent inflation attacks
#[tokio::test]
async fn test_rounding_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let attacker = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    ft_storage_deposit(&usdt, &attacker).await?;
    vault_storage_deposit(&vault, &alice).await?;
    vault_storage_deposit(&vault, &attacker).await?;

    ft_transfer(&usdt, &owner, &alice, 100_000_000).await?;
    ft_transfer(&usdt, &owner, &attacker, 100_000_000).await?;

    // Alice makes first deposit
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    let alice_initial_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_initial_shares.0, 1000);

    // Attacker tries inflation attack by depositing small amount
    ft_transfer_call_deposit(&usdt, &vault, &attacker, 1, None, None, None, None).await?;

    let attacker_shares = vault_balance_of(&vault, &alice, &attacker).await?;
    let total_supply = vault_total_supply(&vault, &alice).await?;
    let total_assets = vault_total_assets(&vault, &alice).await?;

    // With inflation resistance, tiny deposits get rejected (0 shares, unused amount returned)
    // This is excellent protection against inflation attacks
    assert_eq!(
        attacker_shares.0, 0,
        "Attacker should receive zero shares due to inflation resistance"
    );
    assert_eq!(total_supply.0, alice_initial_shares.0); // No change in supply
    assert_eq!(total_assets.0, 1000); // No change in assets (deposit was rejected)

    // Since attacker got 0 shares, they have no claimable assets
    let attacker_claimable = vault_convert_to_assets(&vault, &alice, attacker_shares.0).await?;
    assert_eq!(
        attacker_claimable.0, 0,
        "Attacker should have no claimable assets since they received 0 shares"
    );

    Ok(())
}

/// Test maximum limits and overflow protection
#[tokio::test]
async fn test_large_amounts() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let large_supply = u128::MAX / 2; // Use large but not max value to avoid overflow
    let usdt = deploy_and_init_mock_ft(&owner, Some(large_supply)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 1_000_000_000_000).await?;

    // Test large deposit
    let large_deposit = 1_000_000_000_000u128;
    ft_transfer_call_deposit(&usdt, &vault, &alice, large_deposit, None, None, None, None).await?;

    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(alice_shares.0, large_deposit);

    let total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(total_assets.0, large_deposit);

    // Test conversions with large numbers (accounting for inflation resistance)
    let shares_converted = vault_convert_to_shares(&vault, &alice, large_deposit / 2).await?;
    // With large amounts and 1:1 ratio after inflation resistance adjustment, should be close
    let expected = (large_deposit / 2) * large_deposit / (large_deposit + 1);
    assert_eq!(shares_converted.0, expected);

    Ok(())
}

/// Test withdrawal with insufficient balance
#[tokio::test]
async fn test_insufficient_balance_withdrawal() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Deposit
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    // Try to withdraw more than available
    let result = vault_withdraw(&vault, &alice, 2000, None, None).await;
    assert!(
        result.is_err(),
        "Should fail when withdrawing more than max_withdraw"
    );
    let error_message = format!("{:?}", result.unwrap_err());
    assert!(
        error_message.contains("Exceeds max withdraw"),
        "Should contain specific 'Exceeds max withdraw' error message, got: {}",
        error_message
    );

    // Try to redeem more shares than owned
    let result = vault_redeem(&vault, &alice, 2000, None, None).await;
    assert!(
        result.is_err(),
        "Should fail when redeeming more than max_redeem"
    );
    let error_message = format!("{:?}", result.unwrap_err());
    assert!(
        error_message.contains("Exceeds max redeem"),
        "Should contain specific 'Exceeds max redeem' error message, got: {}",
        error_message
    );

    Ok(())
}

/// Test zero amount operations
#[tokio::test]
async fn test_zero_amount_operations() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // First make a normal deposit
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    // Test zero conversions
    let zero_shares = vault_convert_to_shares(&vault, &alice, 0).await?;
    assert_eq!(zero_shares.0, 0);

    let zero_assets = vault_convert_to_assets(&vault, &alice, 0).await?;
    assert_eq!(zero_assets.0, 0);

    // Try zero withdrawal (should fail)
    let result = vault_withdraw(&vault, &alice, 0, None, None).await;
    assert!(result.is_err(), "Should fail when withdrawing zero assets");

    // Try zero redeem (should fail)
    let result = vault_redeem(&vault, &alice, 0, None, None).await;
    assert!(result.is_err(), "Should fail when redeeming zero shares");

    Ok(())
}

/// Test deposit slippage protection failure when min_shares requirement cannot be met
#[tokio::test]
async fn test_deposit_slippage_protection_failure() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // First, make a successful deposit to establish a non-empty vault
    let normal_deposit = 500u128;
    ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        normal_deposit,
        None,
        None,
        None,
        None,
    )
    .await?;

    // Test failed deposit with unreasonably high min_shares requirement
    let deposit_amount = 1000u128;
    let min_shares = 2000u128; // Unreasonable requirement - more shares than possible

    let used_amount = ft_transfer_call_deposit(
        &usdt,
        &vault,
        &alice,
        deposit_amount,
        None,             // receiver_id
        Some(min_shares), // min_shares
        None,             // max_shares
        None,             // memo
    )
    .await?;

    // ft_transfer_call returns the USED amount - when slippage protection triggers,
    // the deposit should be rejected entirely, so 0 tokens should be used
    assert_eq!(
        used_amount.0, 0,
        "No tokens should be used when slippage protection triggers"
    );

    // Verify no shares were minted due to failed slippage protection (beyond the normal deposit)
    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?.0;
    assert_eq!(
        alice_shares,
        500, // Only the normal deposit shares
        "Alice should only have shares from the successful deposit"
    );

    // Verify only normal deposit assets were deposited
    let total_assets = vault_total_assets(&vault, &alice).await?.0;
    assert_eq!(
        total_assets,
        500, // Only the normal deposit
        "Vault should only have assets from the successful deposit"
    );

    // Verify Alice still has the remaining tokens (original 10000 - 500 successful deposit = 9500)
    let alice_usdt_balance = ft_balance_of(&usdt, &alice).await?;
    assert_eq!(
        alice_usdt_balance, 9500,
        "Alice should have her remaining tokens after one successful and one failed deposit"
    );

    Ok(())
}

/// Test max_shares capping functionality
#[tokio::test]
async fn test_max_shares_capping() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Deposit with max_shares limit
    let deposit_amount = 1000u128;
    let max_shares = 700u128; // Less than what would normally be minted (1000 shares for 1000 assets)

    let used_amount = ft_transfer_call_deposit(
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

    // Verify exact shares were minted according to max_shares limit
    let alice_shares = vault_balance_of(&vault, &alice, &alice).await?;
    assert_eq!(
        alice_shares.0, max_shares,
        "Alice should have exactly max_shares amount of vault tokens"
    );

    // Verify only the used amount was deposited as assets
    let total_assets = vault_total_assets(&vault, &alice).await?;
    assert_eq!(
        total_assets.0, used_amount.0,
        "Total vault assets should equal the amount actually used"
    );

    // The used amount should match the assets equivalent of max_shares
    // We allow ±1 rounding difference because:
    // - Deposit uses Rounding::Down for convert_to_assets (conservative)
    // - Preview uses different calculation path
    // This prevents inflation attacks while allowing minimal acceptable rounding
    let expected_used = vault_convert_to_assets(&vault, &alice, max_shares).await?.0;
    assert!(
        used_amount.0 >= expected_used.saturating_sub(1) && used_amount.0 <= expected_used + 1,
        "Used amount should be within ±1 of assets equivalent of max_shares due to rounding modes (got {}, expected {})",
        used_amount.0, expected_used
    );

    // Verify Alice received refund for unused portion
    let alice_balance = ft_balance_of(&usdt, &alice).await?;
    let expected_balance = 10000 - used_amount.0; // Original 10000 minus what was actually used
    assert_eq!(
        alice_balance, expected_balance,
        "Alice should have received refund for unused tokens"
    );

    Ok(())
}

/// Test edge case with very small deposits and withdrawals
#[tokio::test]
async fn test_dust_amounts() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Make normal deposit first
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    // Test very small deposit (dust) - with inflation resistance, might be rejected
    let dust_amount = 1u128;
    let used_amount =
        ft_transfer_call_deposit(&usdt, &vault, &alice, dust_amount, None, None, None, None)
            .await?;

    let alice_shares_after = vault_balance_of(&vault, &alice, &alice).await?;
    let total_supply_after = vault_total_supply(&vault, &alice).await?;
    let total_assets_after = vault_total_assets(&vault, &alice).await?;
    let alice_balance_after = ft_balance_of(&usdt, &alice).await?;

    // Store initial state values for comparison
    let initial_shares = 1000u128;
    let initial_assets = 1000u128;
    let initial_supply = 1000u128;
    let initial_balance = 9000u128; // 10000 - 1000 used in first deposit

    // Check what actually happened - dust deposit should be rejected due to inflation resistance
    // With the current implementation, 1 token deposit after a 1000 token deposit should be rejected
    assert_eq!(
        used_amount.0, 0,
        "Dust deposit of 1 token should be rejected due to inflation resistance (used=0)"
    );

    // Verify vault state remains unchanged after rejected dust deposit
    assert_eq!(
        alice_shares_after.0, initial_shares,
        "Alice should still have exactly 1000 shares after rejected dust deposit"
    );
    assert_eq!(
        total_assets_after.0, initial_assets,
        "Vault should still have exactly 1000 assets after rejected dust deposit"
    );
    assert_eq!(
        total_supply_after.0, initial_supply,
        "Total share supply should remain 1000 after rejected dust deposit"
    );

    // Verify Alice's balance remains unchanged (dust was returned)
    assert_eq!(
        alice_balance_after, initial_balance,
        "Alice should have 9000 tokens after dust deposit rejection (got refunded)"
    );

    // Test conversion functions with dust amounts - verify inflation resistance
    let dust_to_shares = vault_convert_to_shares(&vault, &alice, dust_amount)
        .await?
        .0;
    let dust_to_assets = vault_convert_to_assets(&vault, &alice, dust_amount)
        .await?
        .0;

    // Verify the mathematical behavior of inflation resistance:
    // With vault state (1000 assets, 1000 shares + inflation resistance adjustment):
    // convert_to_shares: (1 * 1000) / (1000 + 1) = 1000/1001 = 0 (rounded down)
    assert_eq!(
        dust_to_shares, 0,
        "1 dust asset should convert to 0 shares due to inflation resistance"
    );

    // convert_to_assets: (1 * (1000 + 1)) / 1000 = 1001/1000 = 1 (rounded down)
    assert_eq!(
        dust_to_assets, 1,
        "1 dust share should convert to 1 asset with inflation adjustment"
    );

    // This asymmetry is intentional - it prevents inflation attacks:
    // - Small asset amounts get rounded down to 0 shares (can't attack)
    // - Small share amounts still have value when converted back (fair to users)

    // Test that zero-amount operations are handled correctly
    let zero_shares_result = vault_convert_to_shares(&vault, &alice, 0).await?.0;
    let zero_assets_result = vault_convert_to_assets(&vault, &alice, 0).await?.0;
    assert_eq!(zero_shares_result, 0, "0 assets should convert to 0 shares");
    assert_eq!(zero_assets_result, 0, "0 shares should convert to 0 assets");

    // Test redeem with 0 shares should fail
    let redeem_zero_result = vault_redeem(&vault, &alice, 0, None, None).await;
    assert!(
        redeem_zero_result.is_err(),
        "Redeeming 0 shares should fail"
    );

    // Test withdraw with 0 assets should fail
    let withdraw_zero_result = vault_withdraw(&vault, &alice, 0, None, None).await;
    assert!(
        withdraw_zero_result.is_err(),
        "Withdrawing 0 assets should fail"
    );

    Ok(())
}

/// Test round-trip property: deposit then withdraw should not create profit
#[tokio::test]
async fn test_deposit_withdraw_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;

    // Initial deposit to establish exchange rate
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    // Record balance before round trip
    let pre_round_trip_balance = ft_balance_of(&usdt, &alice).await?;

    // Perform round trip: deposit then immediate withdraw
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

    let shares_received = vault_balance_of(&vault, &alice, &alice).await?.0 - 1000; // Subtract initial shares

    // Immediate withdrawal
    vault_redeem(&vault, &alice, shares_received, None, None).await?;

    // Check round-trip property: should not gain profit (small loss acceptable due to rounding)
    let final_balance = ft_balance_of(&usdt, &alice).await?;
    let expected_balance = pre_round_trip_balance;

    // Round trip should not create profit (loss <= 1 token due to inflation resistance rounding)
    assert!(
        final_balance <= expected_balance,
        "Round trip should not create profit: got {}, expected {}",
        final_balance,
        expected_balance
    );
    assert!(
        expected_balance - final_balance <= 1,
        "Round trip loss should be minimal (≤1 due to rounding): lost {}",
        expected_balance - final_balance
    );

    Ok(())
}

/// Test that unauthorized transfers to vault are handled correctly
#[tokio::test]
async fn test_unauthorized_asset_transfer() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let fake_owner = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&owner, Some(1_000_000u128)).await?;
    let fake_token = deploy_and_init_mock_ft(&fake_owner, Some(1_000_000u128)).await?;
    let vault = deploy_and_init_vault(&owner, &usdt, "USDT Vault", "vUSDT").await?;

    // Setup accounts
    ft_storage_deposit(&usdt, &alice).await?;
    ft_storage_deposit(&fake_token, &alice).await?;
    vault_storage_deposit(&vault, &alice).await?;
    ft_transfer(&usdt, &owner, &alice, 10000).await?;
    ft_transfer(&fake_token, &fake_owner, &alice, 10000).await?;

    // Try to deposit wrong token - should fail
    let result =
        ft_transfer_call_deposit(&fake_token, &vault, &alice, 1000, None, None, None, None).await;
    assert!(
        result.is_err(),
        "Should reject deposits from unauthorized token contracts"
    );
    let error_message = format!("{:?}", result.unwrap_err());
    // The error could be about unregistered account or unauthorized token
    assert!(
        error_message.contains("Only the underlying asset can be deposited") ||
        error_message.contains("is not registered"),
        "Should contain either unauthorized token or unregistered account error, got: {}",
        error_message
    );

    Ok(())
}

/// Test withdrawal rollback on transfer failure
#[tokio::test]
async fn test_withdrawal_rollback_mechanism() -> Result<(), Box<dyn std::error::Error>> {
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
    ft_transfer_call_deposit(&usdt, &vault, &alice, 1000, None, None, None, None).await?;

    let initial_shares = vault_balance_of(&vault, &alice, &alice).await?.0;
    let initial_total_assets = vault_total_assets(&vault, &alice).await?.0;
    let initial_total_supply = vault_total_supply(&vault, &alice).await?.0;

    // Try to withdraw to non-existent account (should trigger rollback)
    let non_existent = worker.dev_create_account().await?;

    // This should complete with rollback due to transfer failure to unregistered account
    let result = vault_redeem(&vault, &alice, 500, Some(&non_existent), None).await?;

    // Rollback should occur, returning 0 assets and restoring all state
    assert_eq!(
        result.0, 0,
        "Rollback should return 0 assets when transfer fails"
    );

    let final_shares = vault_balance_of(&vault, &alice, &alice).await?.0;
    let final_total_assets = vault_total_assets(&vault, &alice).await?.0;
    let final_total_supply = vault_total_supply(&vault, &alice).await?.0;

    // State should be completely restored after rollback
    assert_eq!(
        final_shares, initial_shares,
        "Shares should be restored on rollback"
    );
    assert_eq!(
        final_total_assets, initial_total_assets,
        "Total assets should be restored on rollback"
    );
    assert_eq!(
        final_total_supply, initial_total_supply,
        "Total supply should be restored on rollback"
    );

    Ok(())
}
