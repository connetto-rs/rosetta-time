#![doc = include_str!("../README.md")]

use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

pub mod diesel_impls;

/// Defines a transparent wrapper of a jiff type with the conversions,
/// formatting and parsing every wrapper of this crate shares.
macro_rules! wrapper {
    ($(#[$attribute:meta])* $name:ident($inner:ty) as $sql_type:ident) => {
        $(#[$attribute])*
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy)]
        #[cfg_attr(
            feature = "diesel",
            derive(diesel::expression::AsExpression, diesel::deserialize::FromSqlRow)
        )]
        #[cfg_attr(feature = "diesel", diesel(sql_type = crate::diesel_impls::$sql_type))]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name($inner);

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<$inner> for $name {
            fn as_ref(&self) -> &$inner {
                &self.0
            }
        }

        impl AsMut<$inner> for $name {
            fn as_mut(&mut self) -> &mut $inner {
                &mut self.0
            }
        }

        impl core::ops::Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl core::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = jiff::Error;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                text.parse().map(Self)
            }
        }
    };
}

wrapper! {
    /// A wrapper around [`jiff::civil::Date`], a calendar date with no time
    /// zone, stored as `DATE`.
    ///
    /// It displays and parses as an ISO 8601 date such as `2026-11-19`.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    Date(jiff::civil::Date) as Date
}

wrapper! {
    /// A wrapper around [`jiff::civil::Time`], a wall-clock time with no time
    /// zone, stored as `TIME`.
    ///
    /// It displays and parses as an ISO 8601 time such as `09:30:00`.
    /// PostgreSQL keeps microseconds, so writing to it floors the time to
    /// the microsecond.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    Time(jiff::civil::Time) as Time
}

wrapper! {
    /// A wrapper around [`jiff::civil::DateTime`], a date and wall-clock time
    /// with no time zone, stored as `TIMESTAMP`.
    ///
    /// It displays and parses as an ISO 8601 datetime such as
    /// `2026-11-19T10:00:00`. PostgreSQL keeps microseconds, so writing to it
    /// floors the datetime to the microsecond.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
    DateTime(jiff::civil::DateTime) as Timestamp
}

wrapper! {
    /// A wrapper around [`jiff::Span`], a duration in calendar and clock
    /// units, stored as `INTERVAL`.
    ///
    /// It displays as an ISO 8601 duration such as `P1DT2H`, and parses that
    /// and jiff's friendly format such as `1 day 2 hours`.
    ///
    /// # Comparison
    ///
    /// A `Span` compares as PostgreSQL compares intervals, counting a year as
    /// 12 months, a month as 30 days, a week as 7 days and a day as 24 hours,
    /// so `1 month` equals `30 days` and `1 day` equals `24 hours`. `Eq`,
    /// `Ord` and `Hash` agree with each other under that rule, and a span read
    /// back from the database compares equal to the span written. This is
    /// coarser than [`jiff::Span::compare`], which needs a reference date to
    /// compare months with days.
    ///
    /// ```
    /// use rosetta_time::Span;
    ///
    /// let month = Span::from(jiff::Span::new().months(1));
    /// let thirty_days = Span::from(jiff::Span::new().days(30));
    /// assert_eq!(month, thirty_days);
    /// assert!(Span::from(jiff::Span::new().hours(25)) > Span::from(jiff::Span::new().days(1)));
    /// ```
    ///
    /// # Storage
    ///
    /// The database holds an interval as months, days and microseconds.
    /// Writing folds years into months, weeks into days and every clock unit
    /// into microseconds, and refuses a span with a sub-microsecond remainder.
    /// Reading gives back a span with exactly those three units, and refuses
    /// an interval whose units carry different signs, such as
    /// `1 mon -1 days`, because a `jiff::Span` has a single sign.
    Span(jiff::Span) as Interval
}

impl DateTime {
    /// Returns the current wall-clock time in UTC, at the precision of the
    /// system clock.
    ///
    /// # Examples
    ///
    /// ```
    /// use rosetta_time::DateTime;
    ///
    /// let now = DateTime::now_utc();
    /// assert!(now.year() >= 2026);
    /// ```
    #[must_use]
    pub fn now_utc() -> Self {
        Self(jiff::tz::Offset::UTC.to_datetime(jiff::Timestamp::now()))
    }
}

impl Date {
    /// Returns the current date in UTC.
    ///
    /// # Examples
    ///
    /// ```
    /// use rosetta_time::{Date, DateTime};
    ///
    /// let today = Date::today_utc();
    /// assert!(today <= DateTime::now_utc().date().into());
    /// ```
    #[must_use]
    pub fn today_utc() -> Self {
        Self(DateTime::now_utc().date())
    }
}

/// Nanoseconds in a day of 24 hours.
const NANOSECONDS_PER_DAY: i128 = 86_400 * 1_000_000_000;

impl Span {
    /// The span in nanoseconds, counting a year as 12 months, a month as 30
    /// days and a week as 7 days, as PostgreSQL's `interval_cmp_value` does.
    fn comparison_key(&self) -> i128 {
        let span = &self.0;
        let months = i128::from(span.get_years()) * 12 + i128::from(span.get_months());
        let days = months * 30 + i128::from(span.get_weeks()) * 7 + i128::from(span.get_days());
        let seconds = (i128::from(span.get_hours()) * 60 + i128::from(span.get_minutes())) * 60
            + i128::from(span.get_seconds());
        days * NANOSECONDS_PER_DAY
            + seconds * 1_000_000_000
            + i128::from(span.get_milliseconds()) * 1_000_000
            + i128::from(span.get_microseconds()) * 1_000
            + i128::from(span.get_nanoseconds())
    }
}

impl PartialEq for Span {
    fn eq(&self, other: &Self) -> bool {
        self.comparison_key() == other.comparison_key()
    }
}

impl Eq for Span {}

impl PartialOrd for Span {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Span {
    fn cmp(&self, other: &Self) -> Ordering {
        self.comparison_key().cmp(&other.comparison_key())
    }
}

impl Hash for Span {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.comparison_key().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn span(text: &str) -> Span {
        text.parse().unwrap()
    }

    #[test]
    fn test_display_uses_iso_8601_and_from_str_reads_it_back() {
        let day: Date = "2026-11-19".parse().unwrap();
        let opens: Time = "09:30".parse().unwrap();
        let at: DateTime = "2026-11-19 10:00:00.5".parse().unwrap();
        let lasts = span("1 day 2 hours");
        assert_eq!(day.to_string(), "2026-11-19");
        assert_eq!(opens.to_string(), "09:30:00");
        assert_eq!(at.to_string(), "2026-11-19T10:00:00.5");
        assert_eq!(lasts.to_string(), "P1DT2H");
        assert_eq!(at.to_string().parse::<DateTime>().unwrap(), at);
        assert!("2026-02-30".parse::<Date>().is_err());
    }

    #[test]
    fn test_now_utc_and_today_utc_agree() {
        let before = Date::today_utc();
        let now = DateTime::now_utc();
        let after = Date::today_utc();
        assert!(before <= Date::from(now.date()) && Date::from(now.date()) <= after);
    }

    #[test]
    fn test_span_compares_as_postgres_intervals() {
        assert_eq!(span("1 month"), span("30 days"));
        assert_eq!(span("1 year"), span("360 days"));
        assert_eq!(span("1 week"), span("168 hours"));
        assert_eq!(span("2 hours"), span("7200 seconds"));
        assert_eq!(span("1 ms"), span("1000 us"));
        assert_ne!(span("1 day"), span("1 day 1 ns"));
        assert!(span("1 month") < span("31 days"));
        assert!(span("-1 day") < span("0 days"));
        assert!(span("-1 month") < span("-29 days"));

        let set: HashSet<Span> = [span("1 month"), span("30 days"), span("720 hours")].into();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_span_comparison_holds_at_the_extremes_of_jiff() {
        let largest = Span::from(
            jiff::Span::new()
                .years(19_998)
                .months(239_976)
                .weeks(1_043_497)
                .days(7_304_484)
                .hours(175_307_616)
                .minutes(10_518_456_960_i64)
                .seconds(631_107_417_600_i64)
                .milliseconds(631_107_417_600_000_i64)
                .microseconds(631_107_417_600_000_000_i64)
                .nanoseconds(i64::MAX),
        );
        let smallest = Span::from(-*largest);
        assert!(smallest < largest);
        assert_eq!(largest.cmp(&largest), Ordering::Equal);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde_uses_the_jiff_text() {
        let day = Date::from(jiff::civil::date(2026, 11, 19));
        assert_eq!(serde_json::to_string(&day).unwrap(), "\"2026-11-19\"");
        let lasts: Span = serde_json::from_str("\"P1DT2H\"").unwrap();
        assert_eq!(lasts, span("26 hours"));
        assert_eq!(serde_json::to_string(&lasts).unwrap(), "\"P1DT2H\"");
    }
}
