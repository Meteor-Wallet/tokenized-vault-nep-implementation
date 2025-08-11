use crate::helper::mock_ft::{
    deploy_and_init_mock_ft, ft_balance_of, ft_storage_deposit, ft_transfer,
};

mod helper;

#[tokio::test]
async fn test_mock_ft_contract_is_working() -> Result<(), Box<dyn std::error::Error>> {
    let worker = near_workspaces::sandbox().await?;

    let trent = worker.dev_create_account().await?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    let usdt = deploy_and_init_mock_ft(&trent, Some(100_000u128)).await?;

    ft_storage_deposit(&usdt, &alice).await?;
    ft_storage_deposit(&usdt, &bob).await?;

    ft_transfer(&usdt, &trent, &alice, 1000).await?;

    let alice_balance_before = ft_balance_of(&usdt, &alice).await?;
    assert_eq!(alice_balance_before, 1000);

    let bob_balance_before = ft_balance_of(&usdt, &bob).await?;
    assert_eq!(bob_balance_before, 0);

    ft_transfer(&usdt, &alice, &bob, 500).await?;

    let alice_balance_after = ft_balance_of(&usdt, &alice).await?;
    assert_eq!(alice_balance_after, 500);

    let bob_balance_after = ft_balance_of(&usdt, &bob).await?;
    assert_eq!(bob_balance_after, 500);

    Ok(())
}
