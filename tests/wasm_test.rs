//! The UTC clock of the wrappers on `wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use rosetta_time::{Date, DateTime};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_now_utc_and_today_utc() {
    let now = DateTime::now_utc();
    assert!(now.year() >= 2026);
    assert!(Date::today_utc() >= Date::from(now.date()));
}
