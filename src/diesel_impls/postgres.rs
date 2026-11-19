#![cfg(feature = "postgres")]
//! The diesel bindings of the wrappers for PostgreSQL, in PostgreSQL's binary
//! encodings.

use diesel::deserialize::{self, FromSql};
use diesel::pg::data_types::{PgDate, PgInterval, PgTime, PgTimestamp};
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types;
use jiff::civil;

use super::interval::IntervalFields;

/// The day PostgreSQL counts dates from.
const EPOCH_DATE: civil::Date = civil::date(2000, 1, 1);
/// The instant PostgreSQL counts timestamps from.
const EPOCH_DATETIME: civil::DateTime = EPOCH_DATE.at(0, 0, 0, 0);

impl FromSql<super::Date, Pg> for crate::Date {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let PgDate(days) = FromSql::<sql_types::Date, Pg>::from_sql(value)?;
        Ok(Self(
            EPOCH_DATE.checked_add(jiff::Span::new().try_days(days)?)?,
        ))
    }
}

impl ToSql<super::Date, Pg> for crate::Date {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let days = self.0.since(EPOCH_DATE)?.get_days();
        ToSql::<sql_types::Date, Pg>::to_sql(&PgDate(days), &mut out.reborrow())
    }
}

impl FromSql<super::Time, Pg> for crate::Time {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let PgTime(microseconds) = FromSql::<sql_types::Time, Pg>::from_sql(value)?;
        Ok(Self(civil::Time::midnight().checked_add(
            jiff::SignedDuration::from_micros(microseconds),
        )?))
    }
}

impl ToSql<super::Time, Pg> for crate::Time {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let microseconds = self.0.duration_since(civil::Time::midnight()).as_micros();
        ToSql::<sql_types::Time, Pg>::to_sql(
            &PgTime(i64::try_from(microseconds)?),
            &mut out.reborrow(),
        )
    }
}

impl FromSql<super::Timestamp, Pg> for crate::DateTime {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let PgTimestamp(microseconds) = FromSql::<sql_types::Timestamp, Pg>::from_sql(value)?;
        Ok(Self(EPOCH_DATETIME.checked_add(
            jiff::SignedDuration::from_micros(microseconds),
        )?))
    }
}

impl ToSql<super::Timestamp, Pg> for crate::DateTime {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let nanoseconds = self.0.duration_since(EPOCH_DATETIME).as_nanos();
        ToSql::<sql_types::Timestamp, Pg>::to_sql(
            &PgTimestamp(i64::try_from(nanoseconds.div_euclid(1_000))?),
            &mut out.reborrow(),
        )
    }
}

impl FromSql<super::Interval, Pg> for crate::Span {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let interval: PgInterval = FromSql::<sql_types::Interval, Pg>::from_sql(value)?;
        let fields = IntervalFields {
            months: interval.months,
            days: interval.days,
            microseconds: interval.microseconds,
        };
        Ok(Self(fields.try_into()?))
    }
}

impl ToSql<super::Interval, Pg> for crate::Span {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let fields = IntervalFields::try_from(self.0)?;
        ToSql::<sql_types::Interval, Pg>::to_sql(
            &PgInterval::new(fields.microseconds, fields.days, fields.months),
            &mut out.reborrow(),
        )
    }
}
