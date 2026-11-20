//! The UTC clock of the wrappers on `wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use rosetta_time::{Date, Time};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_today_utc() {
    assert!(Date::today_utc().year() >= 2026);
}

#[wasm_bindgen_test]
fn test_now_utc() {
    let now = Time::now_utc();
    let later = jiff::tz::Offset::UTC
        .to_datetime(jiff::Timestamp::now())
        .time();
    assert!(*now <= later || later.hour() == 0);
}
