//! Round trips of every wrapper through SQLite, and agreement with the text
//! diesel writes for `chrono` and SQLite writes for its own defaults.
#![cfg(feature = "sqlite")]

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use rosetta_time::{Date, Span, Time};

diesel::table! {
    /// A table with one column of each wrapper.
    events (id) {
        /// The primary key.
        id -> Integer,
        /// A `DATE` column.
        day -> rosetta_time::diesel_impls::Date,
        /// A `TIME` column.
        opens -> rosetta_time::diesel_impls::Time,
        /// An `INTERVAL` column.
        lasts -> Nullable<rosetta_time::diesel_impls::Interval>,
    }
}

/// The same table, read as the text SQLite holds.
mod text {
    diesel::table! {
        /// The text of the `events` table.
        #[sql_name = "events"]
        events (id) {
            /// The primary key.
            id -> Integer,
            /// The text of `day`.
            day -> Text,
            /// The text of `opens`.
            opens -> Text,
            /// The text of `lasts`.
            lasts -> Nullable<Text>,
        }
    }
}

/// The same table, through diesel's own SQL types for `chrono`.
mod chrono_types {
    diesel::table! {
        /// The `events` table as chrono-backed code declares it.
        #[sql_name = "events"]
        events (id) {
            /// The primary key.
            id -> Integer,
            /// `day` as a `chrono::NaiveDate`.
            day -> Date,
            /// `opens` as a `chrono::NaiveTime`.
            opens -> Time,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, Debug, PartialEq, Clone, Copy)]
#[diesel(table_name = events)]
struct Event {
    id: i32,
    day: Date,
    opens: Time,
    lasts: Option<Span>,
}

fn connection() -> SqliteConnection {
    let mut connection = SqliteConnection::establish(":memory:").unwrap();
    diesel::sql_query(
        "CREATE TABLE events (id INTEGER PRIMARY KEY, \
         day TEXT NOT NULL DEFAULT CURRENT_DATE, \
         opens TEXT NOT NULL DEFAULT CURRENT_TIME, \
         lasts TEXT)",
    )
    .execute(&mut connection)
    .unwrap();
    connection
}

/// An event on the date and at the time of `at`, an ISO 8601 datetime.
fn event(id: i32, at: &str, lasts: Option<&str>) -> Event {
    let at: jiff::civil::DateTime = at.parse().unwrap();
    Event {
        id,
        day: at.date().into(),
        opens: at.time().into(),
        lasts: lasts.map(|text| text.parse().unwrap()),
    }
}

fn insert(connection: &mut SqliteConnection, event: &Event) -> QueryResult<usize> {
    diesel::insert_into(events::table)
        .values(event)
        .execute(connection)
}

fn read(connection: &mut SqliteConnection, id: i32) -> QueryResult<Event> {
    events::table
        .find(id)
        .select(Event::as_select())
        .first(connection)
}

fn read_text(connection: &mut SqliteConnection, id: i32) -> (String, String) {
    text::events::table
        .find(id)
        .select((text::events::day, text::events::opens))
        .first(connection)
        .unwrap()
}

fn set_lasts_text(connection: &mut SqliteConnection, id: i32, lasts: &str) {
    diesel::update(text::events::table.find(id))
        .set(text::events::lasts.eq(lasts))
        .execute(connection)
        .unwrap();
}

#[test]
fn test_sqlite_round_trip_of_every_wrapper() {
    let mut connection = connection();
    let events = [
        event(1, "2026-11-19T10:00:00", Some("P1DT2H")),
        event(2, "1999-12-31T23:59:59.123456789", None),
        event(3, "-000005-01-01T00:00:00.5", Some("-P14M3DT0.000001S")),
        event(4, "9999-12-31T23:59:59.999999999", Some("PT0S")),
    ];
    for event in &events {
        insert(&mut connection, event).unwrap();
        assert_eq!(read(&mut connection, event.id).unwrap(), *event);
    }
}

#[test]
fn test_sqlite_writes_and_reads_the_text_diesel_uses_for_chrono() {
    let mut connection = connection();
    let datetimes = [
        "2026-11-19T10:00:00",
        "2026-11-19T10:00:00.5",
        "2026-11-19T10:00:00.123456",
        "2026-11-19T10:00:00.123456789",
        "0001-01-01T00:00:00",
        "-000005-01-01T00:00:00",
    ];
    for (id, datetime) in (1..).zip(datetimes) {
        let written = event(id, datetime, None);
        insert(&mut connection, &written).unwrap();
        let chrono_datetime: chrono::NaiveDateTime =
            datetime.replace("-000005", "-0005").parse().unwrap();
        diesel::insert_into(chrono_types::events::table)
            .values((
                chrono_types::events::id.eq(id + 100),
                chrono_types::events::day.eq(chrono_datetime.date()),
                chrono_types::events::opens.eq(chrono_datetime.time()),
            ))
            .execute(&mut connection)
            .unwrap();

        assert_eq!(
            read_text(&mut connection, id),
            read_text(&mut connection, id + 100)
        );
        assert_eq!(
            read(&mut connection, id + 100).unwrap(),
            Event {
                id: id + 100,
                ..written
            }
        );
        let read_by_chrono: (chrono::NaiveDate, chrono::NaiveTime) = chrono_types::events::table
            .find(id)
            .select((chrono_types::events::day, chrono_types::events::opens))
            .first(&mut connection)
            .unwrap();
        assert_eq!(
            read_by_chrono,
            (chrono_datetime.date(), chrono_datetime.time())
        );
    }
}

#[test]
fn test_sqlite_reads_its_own_current_date_and_time() {
    let mut connection = connection();
    let before = Date::today_utc();
    diesel::insert_into(events::table)
        .values(events::id.eq(1))
        .execute(&mut connection)
        .unwrap();
    let after = Date::today_utc();
    let read = read(&mut connection, 1).unwrap();
    assert!(before <= read.day && read.day <= after);
    assert_eq!(read.opens.subsec_nanosecond(), 0);
}

#[test]
fn test_sqlite_stores_spans_as_postgres_prints_them() {
    let mut connection = connection();
    for (id, span, text) in [
        (1, "P1Y2M3W4DT5H6M7.5S", "1 year 2 mons 25 days 05:06:07.5"),
        (2, "-P1D", "-1 days"),
        (3, "PT0S", "00:00:00"),
        (4, "PT100H", "100:00:00"),
    ] {
        insert(
            &mut connection,
            &event(id, "2026-11-19T10:00:00", Some(span)),
        )
        .unwrap();
        let stored: Option<String> = text::events::table
            .find(id)
            .select(text::events::lasts)
            .first(&mut connection)
            .unwrap();
        assert_eq!(stored.as_deref(), Some(text));
    }
    let lasts = read(&mut connection, 1).unwrap().lasts.unwrap();
    assert_eq!(
        lasts.fieldwise(),
        jiff::Span::new()
            .months(14)
            .days(25)
            .microseconds(18_367_500_000_i64)
    );
}

#[test]
fn test_sqlite_reads_iso_8601_spans_and_refuses_mixed_signs() {
    let mut connection = connection();
    insert(&mut connection, &event(1, "2026-11-19T10:00:00", None)).unwrap();

    set_lasts_text(&mut connection, 1, "PT15M");
    let lasts = read(&mut connection, 1).unwrap().lasts.unwrap();
    assert_eq!(
        lasts.fieldwise(),
        jiff::Span::new().microseconds(900_000_000)
    );

    set_lasts_text(&mut connection, 1, "1 mon -1 days");
    assert!(matches!(
        read(&mut connection, 1),
        Err(diesel::result::Error::DeserializationError(_))
    ));
}

#[test]
fn test_sqlite_refuses_a_span_with_a_sub_microsecond_remainder() {
    let mut connection = connection();
    let refused = insert(
        &mut connection,
        &event(1, "2026-11-19T10:00:00", Some("PT0.0000015S")),
    );
    assert!(matches!(
        refused,
        Err(diesel::result::Error::SerializationError(_))
    ));
}
