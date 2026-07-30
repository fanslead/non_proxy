use nonproxy_storage::{PolicyDatabase, StorageError};

#[test]
fn provider_generation_is_persistent_and_monotonic() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("Provider 代数测试数据库打开失败: {database:?}");
    };

    assert!(matches!(
        database.providers().current_generation("transparent-proxy"),
        Ok(None)
    ));
    assert!(matches!(
        database.providers().next_generation("transparent-proxy"),
        Ok(1)
    ));
    assert!(matches!(
        database.providers().next_generation("transparent-proxy"),
        Ok(2)
    ));
    assert!(matches!(
        database.providers().current_generation("transparent-proxy"),
        Ok(Some(2))
    ));
    assert!(matches!(
        database.providers().next_generation("dns-proxy"),
        Ok(1)
    ));
}

#[test]
fn provider_generation_rejects_unbounded_identifier() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("Provider 标识测试数据库打开失败: {database:?}");
    };

    assert!(matches!(
        database.providers().next_generation("../provider"),
        Err(StorageError::CorruptData {
            field: "provider_generation.provider_id"
        })
    ));
}
