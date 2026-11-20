#![cfg(feature = "diesel")]
//! The diesel SQL types of the wrappers, each carrying its PostgreSQL oid and
//! its SQLite storage class, so one `diesel::table!` serves both backends.

mod interval;
mod postgres;
mod sqlite;

/// Defines the SQL type of one wrapper.
macro_rules! sql_type {
    ($(#[$attribute:meta])* $name:ident, oid = $oid:literal, array_oid = $array_oid:literal) => {
        $(#[$attribute])*
        #[derive(
            Debug, Clone, Copy, Default, diesel::query_builder::QueryId, diesel::sql_types::SqlType,
        )]
        #[cfg_attr(
            feature = "postgres",
            diesel(postgres_type(oid = $oid, array_oid = $array_oid))
        )]
        #[cfg_attr(feature = "sqlite", diesel(sqlite_type(name = "Text")))]
        pub struct $name;
    };
}

sql_type! {
    /// The SQL type of [`crate::Date`].
    ///
    /// PostgreSQL's `date`, sent as days since 2000-01-01. SQLite `TEXT`
    /// holding `YYYY-MM-DD`, as diesel writes `chrono::NaiveDate`.
    Date, oid = 1082, array_oid = 1182
}

sql_type! {
    /// The SQL type of [`crate::Time`].
    ///
    /// PostgreSQL's `time`, sent as microseconds since midnight. SQLite
    /// `TEXT` holding `HH:MM:SS` with 3, 6 or 9 fractional digits when the
    /// fraction is not zero, as diesel writes `chrono::NaiveTime`.
    Time, oid = 1083, array_oid = 1183
}

sql_type! {
    /// The SQL type of [`crate::Span`].
    ///
    /// PostgreSQL's `interval`, sent as months, days and microseconds. SQLite
    /// `TEXT` holding the same three counts as PostgreSQL prints them under
    /// its default `IntervalStyle`, such as `1 year 2 mons 3 days 04:05:06.5`.
    /// Reading SQLite also accepts an ISO 8601 duration such as `P1DT2H`.
    ///
    /// It does not implement [`diesel::sql_types::SqlOrd`], because SQLite
    /// orders that text alphabetically, which disagrees with the order of
    /// the intervals.
    Interval, oid = 1186, array_oid = 1187
}

// The text written for these two orders alphabetically as the values do, for
// the years 0 to 9999.
impl diesel::sql_types::SqlOrd for Date {}
impl diesel::sql_types::SqlOrd for Time {}

/// Declares `$name ± Interval`, typed as PostgreSQL types it. `diesel::table!`
/// gives `+` and `-` to every column whose SQL type is named `Date` or `Time`,
/// so these impls must exist for those columns to compile. PostgreSQL answers
/// `date + interval` with a `timestamp`, which this crate has no wrapper for,
/// so that expression is typed as diesel's own `Timestamp`. SQLite has no
/// interval arithmetic, so these expressions only mean something on
/// PostgreSQL.
macro_rules! interval_arithmetic {
    ($($name:ident => $output:ty),*) => {
        $(
            impl diesel::sql_types::ops::Add for $name {
                type Rhs = Interval;
                type Output = $output;
            }

            impl diesel::sql_types::ops::Sub for $name {
                type Rhs = Interval;
                type Output = $output;
            }
        )*
    };
}

interval_arithmetic!(Date => diesel::sql_types::Timestamp, Time => Time);
