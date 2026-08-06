//! Bridge between `jiff` (application time) and `time` (sqlx Postgres
//! `timestamptz` decode/encode). The DB layer speaks `time::OffsetDateTime`;
//! the rest of the app speaks `jiff`. Convert at the boundary.

use jiff::Timestamp;

pub fn now() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc()
}

pub fn from_jiff(t: Timestamp) -> time::OffsetDateTime {
    let nanos = t.as_nanosecond();
    time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .expect("jiff timestamp fits in time::OffsetDateTime range")
}

pub fn to_jiff(t: time::OffsetDateTime) -> Timestamp {
    let nanos = t.unix_timestamp_nanos();
    Timestamp::from_nanosecond(nanos).unwrap_or_else(|_| {
        // Fallback for out-of-range dates (extremely unlikely in this app).
        Timestamp::default()
    })
}
