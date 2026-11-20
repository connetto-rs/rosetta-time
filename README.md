# rosetta-time

[![CI](https://github.com/connetto-rs/rosetta-time/actions/workflows/ci.yml/badge.svg)](https://github.com/connetto-rs/rosetta-time/actions/workflows/ci.yml)
[![Security Audit](https://github.com/connetto-rs/rosetta-time/actions/workflows/security.yml/badge.svg)](https://github.com/connetto-rs/rosetta-time/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Codecov](https://codecov.io/gh/connetto-rs/rosetta-time/branch/main/graph/badge.svg)](https://codecov.io/gh/connetto-rs/rosetta-time)
[![Crates.io](https://img.shields.io/crates/v/rosetta-time.svg)](https://crates.io/crates/rosetta-time)
[![Docs.rs](https://docs.rs/rosetta-time/badge.svg)](https://docs.rs/rosetta-time)

Wrappers of [`jiff`](https://crates.io/crates/jiff)'s `civil::Date`, `civil::Time` and `Span` with serde support and one diesel SQL type each that serves both PostgreSQL and SQLite.

| Wrapper | SQL type                               | PostgreSQL | SQLite `TEXT`                     |
|---------|----------------------------------------|------------|-----------------------------------|
| `Date`  | `rosetta_time::diesel_impls::Date`     | `date`     | `2026-11-19`                      |
| `Time`  | `rosetta_time::diesel_impls::Time`     | `time`     | `09:30:00.500`                    |
| `Span`  | `rosetta_time::diesel_impls::Interval` | `interval` | `1 year 2 mons 3 days 04:05:06.5` |

PostgreSQL uses its binary encodings. The SQLite text of dates and times is what diesel writes for `chrono`'s naive types, and the text of a span is what PostgreSQL prints for the interval. Both databases keep an interval as months, days and microseconds, and `Span` compares the way PostgreSQL compares intervals, so `1 month` equals `30 days`.

## Features

* **`diesel`** enables the SQL types, **`postgres`** and **`sqlite`** their backends.
* **`serde`** serializes each wrapper as jiff's ISO 8601 text.
* **`wasm`** makes `Date::today_utc()` and `Time::now_utc()` work on `wasm32-unknown-unknown`.

## Example

```rust
use rosetta_time::{Date, Span, Time};

let day: Date = jiff::civil::date(2026, 11, 19).into();
let opens: Time = jiff::civil::time(9, 30, 0, 0).into();
let lasts: Span = jiff::Span::new().days(1).hours(2).into();

assert_eq!(day.year(), 2026);
assert_eq!(opens.to_string(), "09:30:00");
assert_eq!(lasts, "PT26H".parse().unwrap());
assert!(Date::today_utc() >= day);
```
