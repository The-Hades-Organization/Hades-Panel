use serde_json::json;

use super::*;

#[test]
fn dur_em_cada_faixa() {
    assert_eq!(dur(None), "—");
    assert_eq!(dur(Some(f64::NAN)), "—");
    assert_eq!(dur(Some(42.4)), "42 s");
    assert_eq!(dur(Some(150.0)), "3 min");
    assert_eq!(dur(Some(3600.0)), "1 h");
    assert_eq!(dur(Some(5400.0)), "1,5 h");
    assert_eq!(dur(Some(3.0 * 86400.0)), "3 d");
}

#[test]
fn gb_e_mb() {
    assert_eq!(gb(None), "—");
    assert_eq!(gb(Some(1.5 * GB)), "1,5 GB");
    assert_eq!(gb(Some(12.4 * GB)), "12 GB");
    assert_eq!(mb(Some(300.0 * MB)), "300 MB");
    assert_eq!(mb(Some(2.0 * GB)), "2,0 GB");
    assert_eq!(mb(None), "—");
}

#[test]
fn usd_ktok_e_milhar() {
    assert_eq!(usd(Some(1234.6)), "US$ 1.235");
    assert_eq!(usd(None), "—");
    assert_eq!(ktok(Some(45_600.0)), "46k");
    assert_eq!(milhar(0), "0");
    assert_eq!(milhar(999), "999");
    assert_eq!(milhar(1_234_567), "1.234.567");
    assert_eq!(milhar(-12_000), "-12.000");
}

#[test]
fn pct1_decimal_e_plural() {
    assert_eq!(pct1(12.0), "12%");
    assert_eq!(pct1(12.34), "12,3%");
    assert_eq!(decimal_pt(2.25), "2,2");
    assert_eq!(plural(1, "lock", "locks"), "1 lock");
    assert_eq!(plural(2, "lock", "locks"), "2 locks");
}

#[test]
fn leitores_de_json() {
    assert_eq!(num(&json!("7")), Some(7.0));
    assert_eq!(num(&json!(null)), None);
    assert_eq!(texto(&json!(12)), "12");
    assert!(sim(&json!(1)));
    assert!(!sim(&json!("")));
    assert!(arr(&json!({})).is_empty());
    assert_eq!(cauda("/home/acme/demo/"), "demo");
}
