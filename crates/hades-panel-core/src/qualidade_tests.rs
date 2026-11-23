use super::*;

#[test]
fn niveis_na_ordem_do_ts() {
    assert_eq!(level_at("bloqueante"), 0);
    assert_eq!(level_at("x"), -1);
    assert!(truthy(Some(&serde_json::json!([1]))));
    assert!(!truthy(Some(&serde_json::json!({}))));
}
