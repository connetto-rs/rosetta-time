#![cfg(any(feature = "postgres", feature = "sqlite"))]
//! The three counts PostgreSQL holds an interval in, which both backends
//! store, and their conversions from and to [`jiff::Span`].

use core::fmt::{self, Display, Formatter};

/// Microseconds in a second.
const MICROSECONDS_PER_SECOND: i64 = 1_000_000;

/// Why a span and an interval could not be converted.
#[derive(Debug, thiserror::Error)]
pub(super) enum IntervalError {
    /// The span holds nanoseconds that do not make a whole microsecond.
    #[error("the span {0} has a sub-microsecond remainder, which an interval cannot hold")]
    SubMicrosecond(jiff::Span),
    /// The interval gives its counts different signs.
    #[error("the interval `{0}` gives its units different signs, which a jiff::Span cannot hold")]
    MixedSigns(IntervalFields),
    /// The interval exceeds what a span holds in one of its units.
    #[error("the interval `{0}` exceeds the range of a jiff::Span")]
    OutOfRange(IntervalFields, #[source] jiff::Error),
    /// The text is neither PostgreSQL's interval output nor ISO 8601.
    #[cfg(feature = "sqlite")]
    #[error(
        "cannot read {0:?} as an interval in PostgreSQL's output format, such as `1 day 02:00:00`"
    )]
    Unreadable(String),
    /// The text starts as an ISO 8601 duration but jiff cannot read it.
    #[cfg(feature = "sqlite")]
    #[error("cannot read {text:?} as an ISO 8601 duration")]
    UnreadableIso {
        /// The text read.
        text: String,
        /// Why jiff refused it.
        #[source]
        source: jiff::Error,
    },
}

/// An interval as PostgreSQL holds it, in months, days and microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct IntervalFields {
    /// Months, with years folded in.
    pub(super) months: i32,
    /// Days, with weeks folded in.
    pub(super) days: i32,
    /// Every clock unit, in microseconds.
    pub(super) microseconds: i64,
}

impl TryFrom<jiff::Span> for IntervalFields {
    type Error = IntervalError;

    fn try_from(span: jiff::Span) -> Result<Self, Self::Error> {
        let nanoseconds = span.get_nanoseconds();
        if nanoseconds % 1_000 != 0 {
            return Err(IntervalError::SubMicrosecond(span));
        }
        // jiff bounds every unit tightly enough that none of these sums overflows.
        let seconds =
            (i64::from(span.get_hours()) * 60 + span.get_minutes()) * 60 + span.get_seconds();
        Ok(Self {
            months: i32::from(span.get_years()) * 12 + span.get_months(),
            days: span.get_weeks() * 7 + span.get_days(),
            microseconds: seconds * MICROSECONDS_PER_SECOND
                + span.get_milliseconds() * 1_000
                + span.get_microseconds()
                + nanoseconds / 1_000,
        })
    }
}

impl TryFrom<IntervalFields> for jiff::Span {
    type Error = IntervalError;

    fn try_from(fields: IntervalFields) -> Result<Self, Self::Error> {
        let signs = [
            i64::from(fields.months).signum(),
            i64::from(fields.days).signum(),
            fields.microseconds.signum(),
        ];
        if signs.contains(&1) && signs.contains(&-1) {
            return Err(IntervalError::MixedSigns(fields));
        }
        jiff::Span::new()
            .try_months(fields.months)
            .and_then(|span| span.try_days(fields.days))
            .and_then(|span| span.try_microseconds(fields.microseconds))
            .map_err(|source| IntervalError::OutOfRange(fields, source))
    }
}

/// Writes the interval as PostgreSQL prints it under its default
/// `IntervalStyle`, such as `1 year 2 mons 3 days 04:05:06.5` or `-1 days`.
///
/// Each count carries its own sign and is plural unless it is exactly one.
/// The clock, whose hours are unbounded, prints only when it is not zero,
/// except for an interval that is zero throughout.
impl Display for IntervalFields {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for (count, singular, plural) in [
            (self.months / 12, "year", "years"),
            (self.months % 12, "mon", "mons"),
            (self.days, "day", "days"),
        ] {
            if count != 0 {
                let unit = if count == 1 { singular } else { plural };
                write!(formatter, "{separator}{count} {unit}")?;
                separator = " ";
            }
        }
        if self.microseconds == 0 && !separator.is_empty() {
            return Ok(());
        }
        let sign = if self.microseconds < 0 { "-" } else { "" };
        let magnitude = self.microseconds.unsigned_abs();
        let seconds = magnitude / MICROSECONDS_PER_SECOND.unsigned_abs();
        write!(
            formatter,
            "{separator}{sign}{:02}:{:02}:{:02}",
            seconds / 3_600,
            seconds % 3_600 / 60,
            seconds % 60
        )?;
        let mut fraction = magnitude % MICROSECONDS_PER_SECOND.unsigned_abs();
        if fraction == 0 {
            return Ok(());
        }
        let mut width = 6;
        while fraction.is_multiple_of(10) {
            fraction /= 10;
            width -= 1;
        }
        write!(formatter, ".{fraction:0width$}")
    }
}

/// Reads PostgreSQL's interval output, the form [`Display`] writes, or an
/// ISO 8601 duration such as `P1DT2H`, which is folded into the three counts.
#[cfg(feature = "sqlite")]
impl core::str::FromStr for IntervalFields {
    type Err = IntervalError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        if trimmed
            .trim_start_matches(['+', '-'])
            .starts_with(['P', 'p'])
        {
            let span: jiff::Span =
                trimmed
                    .parse()
                    .map_err(|source| IntervalError::UnreadableIso {
                        text: text.to_owned(),
                        source,
                    })?;
            return Self::try_from(span);
        }
        parse_postgres_output(trimmed).ok_or_else(|| IntervalError::Unreadable(text.to_owned()))
    }
}

/// Reads a sequence of `N year(s)`, `N mon(s)`, `N day(s)` and
/// `[-]H:MM:SS[.ffffff]`, answering `None` for anything else or an overflow.
#[cfg(feature = "sqlite")]
fn parse_postgres_output(text: &str) -> Option<IntervalFields> {
    if text.is_empty() {
        return None;
    }
    let mut fields = IntervalFields::default();
    let mut tokens = text.split_whitespace();
    while let Some(token) = tokens.next() {
        if token.contains(':') {
            fields.microseconds = fields.microseconds.checked_add(parse_clock(token)?)?;
            continue;
        }
        let count: i32 = token.parse().ok()?;
        let (counter, per_unit) = match tokens.next()? {
            "year" | "years" => (&mut fields.months, 12),
            "mon" | "mons" => (&mut fields.months, 1),
            "day" | "days" => (&mut fields.days, 1),
            _ => return None,
        };
        *counter = counter.checked_add(count.checked_mul(per_unit)?)?;
    }
    Some(fields)
}

/// Reads `[-]H:MM:SS[.ffffff]` as microseconds, with the hours unbounded.
#[cfg(feature = "sqlite")]
fn parse_clock(token: &str) -> Option<i64> {
    let (negative, clock) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token),
    };
    let mut parts = clock.split(':');
    let (hours, minutes, seconds) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || minutes.len() != 2 {
        return None;
    }
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    if seconds.len() != 2 || fraction.len() > 6 {
        return None;
    }
    let (minutes, seconds) = (digits(minutes)?, digits(seconds)?);
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        digits(fraction)? * 10_i64.pow(6 - u32::try_from(fraction.len()).ok()?)
    };
    let magnitude = digits(hours)?
        .checked_mul(3_600)?
        .checked_add(minutes * 60 + seconds)?
        .checked_mul(MICROSECONDS_PER_SECOND)?
        .checked_add(fraction)?;
    Some(if negative { -magnitude } else { magnitude })
}

/// Reads a non-empty run of ASCII digits.
#[cfg(feature = "sqlite")]
fn digits(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(months: i32, days: i32, microseconds: i64) -> IntervalFields {
        IntervalFields {
            months,
            days,
            microseconds,
        }
    }

    #[test]
    fn test_span_folds_into_three_counts() {
        let span = jiff::Span::new()
            .years(1)
            .months(2)
            .weeks(1)
            .days(3)
            .hours(4)
            .minutes(5)
            .seconds(6)
            .milliseconds(7)
            .microseconds(8)
            .nanoseconds(9_000);
        assert_eq!(
            IntervalFields::try_from(span).unwrap(),
            fields(14, 10, 14_706_007_017)
        );
        let negative = IntervalFields::try_from(-jiff::Span::new().years(1).hours(1)).unwrap();
        assert_eq!(negative, fields(-12, 0, -3_600_000_000));
    }

    #[test]
    fn test_span_with_a_sub_microsecond_remainder_is_refused() {
        let span = jiff::Span::new().microseconds(1).nanoseconds(1);
        assert!(matches!(
            IntervalFields::try_from(span),
            Err(IntervalError::SubMicrosecond(_))
        ));
        assert!(IntervalFields::try_from(jiff::Span::new().nanoseconds(-2_000)).is_ok());
    }

    #[test]
    fn test_counts_become_a_span_with_exactly_those_units() {
        let span = jiff::Span::try_from(fields(-14, -3, -5)).unwrap();
        assert_eq!(
            span.fieldwise(),
            jiff::Span::new().months(-14).days(-3).microseconds(-5)
        );
    }

    #[test]
    fn test_counts_with_mixed_signs_are_refused() {
        for mixed in [fields(1, -1, 0), fields(0, -1, 1), fields(-1, 0, 1)] {
            assert!(matches!(
                jiff::Span::try_from(mixed),
                Err(IntervalError::MixedSigns(_))
            ));
        }
    }

    #[test]
    fn test_counts_beyond_jiff_are_refused() {
        assert!(matches!(
            jiff::Span::try_from(fields(0, 0, i64::MAX)),
            Err(IntervalError::OutOfRange(..))
        ));
    }

    #[test]
    fn test_display_prints_postgres_output() {
        for (interval, text) in [
            (
                fields(14, 3, 14_706_500_000),
                "1 year 2 mons 3 days 04:05:06.5",
            ),
            (fields(0, -1, 0), "-1 days"),
            (fields(0, 0, 900_000_000), "00:15:00"),
            (fields(0, 0, 0), "00:00:00"),
            (fields(-14, 0, 0), "-1 years -2 mons"),
            (
                fields(1, 1, -360_000_000_001),
                "1 mon 1 day -100:00:00.000001",
            ),
            (fields(24, 2, 1), "2 years 2 days 00:00:00.000001"),
        ] {
            assert_eq!(interval.to_string(), text);
        }
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_from_str_reads_what_display_prints() {
        for interval in [
            fields(14, 3, 14_706_500_000),
            fields(0, -1, 0),
            fields(0, 0, 0),
            fields(-14, 0, 0),
            fields(1, 1, -360_000_000_001),
            fields(i32::MAX, i32::MIN, i64::MIN + 1),
        ] {
            assert_eq!(
                interval.to_string().parse::<IntervalFields>().unwrap(),
                interval
            );
        }
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_from_str_folds_iso_8601() {
        assert_eq!(
            "P1Y1WT2H".parse::<IntervalFields>().unwrap(),
            fields(12, 7, 7_200_000_000)
        );
        assert_eq!("-P1D".parse::<IntervalFields>().unwrap(), fields(0, -1, 0));
        assert!(matches!(
            "P1X".parse::<IntervalFields>(),
            Err(IntervalError::UnreadableIso { .. })
        ));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_from_str_refuses_other_text() {
        for text in [
            "",
            "1 day 2",
            "1 week",
            "1 days 1:2:03",
            "00:60:00",
            "00:00:00.1234567",
            "1:00",
            "99999999999 years",
            "+-1:00:00",
        ] {
            assert!(
                matches!(
                    text.parse::<IntervalFields>(),
                    Err(IntervalError::Unreadable(_))
                ),
                "{text:?}"
            );
        }
    }
}
