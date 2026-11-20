#![cfg(feature = "sqlite")]
//! The diesel bindings of the wrappers for SQLite, as `TEXT` in the forms
//! diesel writes for `chrono`'s naive types, so rows written through either
//! stay readable through the other.

use core::fmt::{self, Display, Formatter};

use diesel::deserialize::{self, FromSql};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sqlite::{Sqlite, SqliteValue};
use jiff::civil;

use super::interval::IntervalFields;

/// The format of [`DateText`], for reading back the years jiff's ISO 8601
/// parser refuses.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// A date as `YYYY-MM-DD`, with a signed year of at least four digits outside
/// 0 to 9999, as chrono's `%F` writes it.
struct DateText(civil::Date);

impl Display for DateText {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let year = self.0.year();
        if (0..=9999).contains(&year) {
            write!(formatter, "{year:04}")?;
        } else {
            write!(formatter, "{year:+05}")?;
        }
        write!(formatter, "-{:02}-{:02}", self.0.month(), self.0.day())
    }
}

/// A time as `HH:MM:SS`, followed by 3, 6 or 9 fractional digits when the
/// fraction is not zero, as chrono's `%T%.f` writes it.
struct TimeText(civil::Time);

impl Display for TimeText {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let time = self.0;
        write!(
            formatter,
            "{:02}:{:02}:{:02}",
            time.hour(),
            time.minute(),
            time.second()
        )?;
        match time.subsec_nanosecond() {
            0 => Ok(()),
            nanoseconds if nanoseconds % 1_000_000 == 0 => {
                write!(formatter, ".{:03}", nanoseconds / 1_000_000)
            }
            nanoseconds if nanoseconds % 1_000 == 0 => {
                write!(formatter, ".{:06}", nanoseconds / 1_000)
            }
            nanoseconds => write!(formatter, ".{nanoseconds:09}"),
        }
    }
}

/// Reads ISO 8601, which covers what diesel and SQLite write, and falls back
/// to `DATE_FORMAT` for the years outside 0 to 9999 that only it spells
/// with four digits.
impl FromSql<super::Date, Sqlite> for crate::Date {
    fn from_sql(mut value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let text = value.read_text();
        let date = text
            .parse()
            .or_else(|error| civil::Date::strptime(DATE_FORMAT, text).map_err(|_| error))?;
        Ok(Self(date))
    }
}

impl ToSql<super::Date, Sqlite> for crate::Date {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(DateText(self.0).to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<super::Time, Sqlite> for crate::Time {
    fn from_sql(mut value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(value.read_text().parse()?))
    }
}

impl ToSql<super::Time, Sqlite> for crate::Time {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(TimeText(self.0).to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<super::Interval, Sqlite> for crate::Span {
    fn from_sql(mut value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let fields: IntervalFields = value.read_text().parse()?;
        Ok(Self(fields.try_into()?))
    }
}

impl ToSql<super::Interval, Sqlite> for crate::Span {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(IntervalFields::try_from(self.0)?.to_string());
        Ok(IsNull::No)
    }
}
