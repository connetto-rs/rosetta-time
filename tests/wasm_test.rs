//! The UTC clock of the wrappers on `wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use rosetta_time::Date;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_today_utc() {
    assert!(Date::today_utc().year() >= 2026);
}
