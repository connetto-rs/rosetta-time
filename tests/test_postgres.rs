//! Round trips of every wrapper through PostgreSQL.
#![cfg(feature = "postgres")]

use diesel::pg::PgConnection;
use diesel::prelude::*;
use rosetta_time::{Date, DateTime, Span, Time};

diesel::table! {
    /// A table with one column of each wrapper.
    events (id) {
        /// The primary key.
        id -> Integer,
        /// A `DATE` column.
        day -> rosetta_time::diesel_impls::Date,
        /// A `TIMESTAMP` column.
        at -> rosetta_time::diesel_impls::Timestamp,
        /// A `TIME` column.
        opens -> rosetta_time::diesel_impls::Time,
        /// An `INTERVAL` column.
        lasts -> Nullable<rosetta_time::diesel_impls::Interval>,
    }
}

#[derive(Queryable, Selectable, Insertable, Debug, PartialEq, Clone, Copy)]
#[diesel(table_name = events)]
struct Event {
    id: i32,
    day: Date,
    at: DateTime,
    opens: Time,
    lasts: Option<Span>,
}

fn connection() -> PgConnection {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let mut connection =
        PgConnection::establish(&database_url).expect("Error connecting to database");
    connection.begin_test_transaction().unwrap();
    diesel::sql_query(
        "CREATE TEMPORARY TABLE events (id INTEGER PRIMARY KEY, day DATE NOT NULL, \
         at TIMESTAMP NOT NULL, opens TIME NOT NULL, lasts INTERVAL)",
    )
    .execute(&mut connection)
    .unwrap();
    connection
}

fn event(id: i32, at: &str, lasts: Option<&str>) -> Event {
    let at: jiff::civil::DateTime = at.parse().unwrap();
    Event {
        id,
        day: at.date().into(),
        at: at.into(),
        opens: at.time().into(),
        lasts: lasts.map(|text| text.parse().unwrap()),
    }
}

fn insert(connection: &mut PgConnection, event: &Event) -> QueryResult<usize> {
    diesel::insert_into(events::table)
        .values(event)
        .execute(connection)
}

fn read(connection: &mut PgConnection, id: i32) -> QueryResult<Event> {
    events::table
        .find(id)
        .select(Event::as_select())
        .first(connection)
}

#[test]
fn test_postgres_round_trip_of_every_wrapper() {
    let mut connection = connection();
    let events = [
        event(1, "2026-11-19T10:00:00", Some("P1DT2H")),
        event(2, "1999-12-31T23:59:59.999999", Some("-P14M3DT0.000001S")),
        event(3, "9999-12-31T23:59:59.999999", None),
        event(4, "-004000-03-01T00:00:00.000001", Some("PT0S")),
    ];
    for event in &events {
        insert(&mut connection, event).unwrap();
        assert_eq!(read(&mut connection, event.id).unwrap(), *event);
    }
}

#[test]
fn test_postgres_floors_datetimes_and_times_to_the_microsecond() {
    let mut connection = connection();
    insert(
        &mut connection,
        &event(1, "1999-12-31T23:59:59.000000999", None),
    )
    .unwrap();
    let read = read(&mut connection, 1).unwrap();
    assert_eq!(read.at.to_string(), "1999-12-31T23:59:59");
    assert_eq!(read.opens.to_string(), "23:59:59");
}

#[test]
fn test_postgres_span_reads_back_as_months_days_and_microseconds() {
    let mut connection = connection();
    insert(
        &mut connection,
        &event(1, "2026-11-19T10:00:00", Some("P1Y2M3W4DT5H6M7.008009S")),
    )
    .unwrap();
    let lasts = read(&mut connection, 1).unwrap().lasts.unwrap();
    assert_eq!(
        lasts.fieldwise(),
        jiff::Span::new()
            .months(14)
            .days(25)
            .microseconds(18_367_008_009_i64)
    );
}

#[test]
fn test_postgres_refuses_a_span_with_a_sub_microsecond_remainder() {
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

#[test]
fn test_postgres_refuses_to_read_an_interval_with_mixed_signs() {
    let mut connection = connection();
    insert(&mut connection, &event(1, "2026-11-19T10:00:00", None)).unwrap();
    diesel::update(events::table.find(1))
        .set(events::lasts.eq(diesel::dsl::sql::<
            diesel::sql_types::Nullable<rosetta_time::diesel_impls::Interval>,
        >("interval '1 mon -1 day'")))
        .execute(&mut connection)
        .unwrap();
    assert!(matches!(
        read(&mut connection, 1),
        Err(diesel::result::Error::DeserializationError(_))
    ));
}

#[test]
fn test_postgres_compares_spans_as_the_crate_does() {
    let mut connection = connection();
    let spans = ["P1M", "P30D", "PT720H", "P29DT23H", "P31D", "-P1D"];
    for (id, span) in (1..).zip(spans) {
        insert(
            &mut connection,
            &event(id, "2026-11-19T10:00:00", Some(span)),
        )
        .unwrap();
    }
    let month: Span = "P1M".parse().unwrap();
    let mut equal_in_postgres: Vec<i32> = events::table
        .filter(events::lasts.eq(month))
        .select(events::id)
        .load(&mut connection)
        .unwrap();
    equal_in_postgres.sort_unstable();
    assert_eq!(equal_in_postgres, [1, 2, 3]);

    let ordered_in_postgres: Vec<Option<Span>> = events::table
        .order_by((events::lasts, events::id))
        .select(events::lasts)
        .load(&mut connection)
        .unwrap();
    let mut ordered_in_rust = ordered_in_postgres.clone();
    ordered_in_rust.sort();
    assert_eq!(ordered_in_postgres, ordered_in_rust);
    assert_eq!(
        ordered_in_rust.first(),
        Some(&Some("-P1D".parse().unwrap()))
    );
}

#[test]
fn test_postgres_adds_and_subtracts_intervals() {
    let mut connection = connection();
    insert(
        &mut connection,
        &event(1, "2026-11-19T10:00:00", Some("P1M1DT2H")),
    )
    .unwrap();
    let lasts = events::lasts.assume_not_null();
    let (day_plus, at_plus, opens_minus): (DateTime, DateTime, Time) = events::table
        .find(1)
        .select((
            events::day + lasts,
            events::at + lasts,
            events::opens - lasts,
        ))
        .first(&mut connection)
        .unwrap();
    assert_eq!(day_plus.to_string(), "2026-12-20T02:00:00");
    assert_eq!(at_plus.to_string(), "2026-12-20T12:00:00");
    assert_eq!(opens_minus.to_string(), "08:00:00");
}
