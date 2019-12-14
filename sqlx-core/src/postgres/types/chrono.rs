use crate::{Decode, Postgres, Encode, HasSqlType, HasTypeMetadata};
use chrono::{NaiveTime, Timelike, NaiveDate, TimeZone, DateTime, NaiveDateTime, Utc, Local, Duration, Date};
use crate::postgres::types::{PostgresTypeMetadata, PostgresTypeFormat};
use crate::encode::IsNull;

use std::convert::TryInto;

use std::mem::size_of;

postgres_metadata!(
    // time
    NaiveTime: PostgresTypeMetadata {
        format: PostgresTypeFormat::Binary,
        oid: 1083,
        array_oid: 1183
    },
    // date
    NaiveDate: PostgresTypeMetadata {
        format: PostgresTypeFormat::Binary,
        oid: 1082,
        array_oid: 1182
    },
    // timestamp
    NaiveDateTime: PostgresTypeMetadata {
        format: PostgresTypeFormat::Binary,
        oid: 1114,
        array_oid: 1115
    },
    // timestamptz
    { Tz: TimeZone } DateTime<Tz>: PostgresTypeMetadata {
        format: PostgresTypeFormat::Binary,
        oid: 1184,
        array_oid: 1185
    },
    // Date<Tz: TimeZone> is not covered as Postgres does not have a "date with timezone" type
);

fn decode<T: Decode<Postgres>>(raw: Option<&[u8]>) -> T {
    Decode::<Postgres>::decode(raw)
}

impl Decode<Postgres> for NaiveTime {
    fn decode(raw: Option<&[u8]>) -> Self {
        let micros: i64 = decode(raw);
        NaiveTime::from_hms(0, 0, 0) + Duration::microseconds(micros)
    }
}

impl Encode<Postgres> for NaiveTime {
    fn encode(&self, buf: &mut Vec<u8>) -> IsNull {
        let micros = (*self - NaiveTime::from_hms(0, 0, 0))
            .num_microseconds()
            .expect("shouldn't overflow");

        Encode::<Postgres>::encode(&micros, buf)
    }

    fn size_hint(&self) -> usize {
        size_of::<i64>()
    }
}

impl Decode<Postgres> for NaiveDate {
    fn decode(raw: Option<&[u8]>) -> Self {
        let days: i32 = decode(raw);
        NaiveDate::from_ymd(2000, 1, 1) + Duration::days(days as i64)
    }
}

impl Encode<Postgres> for NaiveDate {
    fn encode(&self, buf: &mut Vec<u8>) -> IsNull {
        let days: i32 = self.signed_duration_since(NaiveDate::from_ymd(2000, 1, 1))
            .num_days()
            .try_into()
            .unwrap_or_else(|_| panic!("NaiveDate out of range for Postgres: {:?}", self));

        Encode::<Postgres>::encode(&days, buf)
    }

    fn size_hint(&self) -> usize {
        size_of::<i32>()
    }
}

impl Decode<Postgres> for NaiveDateTime {
    fn decode(raw: Option<&[u8]>) -> Self {
        let micros: i64 = decode(raw);
        postgres_epoch().naive_utc()
            .checked_add_signed(Duration::microseconds(micros))
            .unwrap_or_else(|| panic!("Postgres timestamp out of range for NaiveDateTime: {:?}", micros))
    }
}

impl Encode<Postgres> for NaiveDateTime {
    fn encode(&self, buf: &mut Vec<u8>) -> IsNull {
        let micros = self.signed_duration_since(postgres_epoch().naive_utc())
            .num_microseconds()
            .unwrap_or_else(|| panic!("NaiveDateTime out of range for Postgres: {:?}", self));

        Encode::<Postgres>::encode(&micros, buf)
    }

    fn size_hint(&self) -> usize {
        size_of::<i64>()
    }
}

impl Decode<Postgres> for DateTime<Utc> {
    fn decode(raw: Option<&[u8]>) -> Self {
        let date_time = <NaiveDateTime as Decode<Postgres>>::decode(raw);
        DateTime::from_utc(date_time, Utc)
    }
}

impl Decode<Postgres> for DateTime<Local> {
    fn decode(raw: Option<&[u8]>) -> Self {
        let date_time = <NaiveDateTime as Decode<Postgres>>::decode(raw);
        Local.from_utc_datetime(&date_time)
    }
}

impl<Tz: TimeZone> Encode<Postgres> for DateTime<Tz> where Tz::Offset: Copy {
    fn encode(&self, buf: &mut Vec<u8>) -> IsNull {
        Encode::<Postgres>::encode(&self.naive_utc(), buf)
    }

    fn size_hint(&self) -> usize {
        size_of::<i64>()
    }
}

fn postgres_epoch() -> DateTime<Utc> {
    Utc.ymd(2000, 1, 1).and_hms(0, 0, 0)
}

#[test]
fn test_encode_datetime() {
    let mut buf = Vec::new();

    let date = postgres_epoch();
    Encode::<Postgres>::encode(&date, &mut buf);
    assert_eq!(buf, [0; 8]);
    buf.clear();

    // one hour past epoch
    let date2 = postgres_epoch() + Duration::hours(1);
    Encode::<Postgres>::encode(&date2, &mut buf);
    assert_eq!(buf, 3_600_000_000i64.to_be_bytes());
    buf.clear();

    // some random date
    let date3: NaiveDateTime = "2019-12-11T11:01:05".parse().unwrap();
    let expected = dbg!((date3 - postgres_epoch().naive_utc()).num_microseconds().unwrap());
    Encode::<Postgres>::encode(&date3, &mut buf);
    assert_eq!(buf, expected.to_be_bytes());
    buf.clear();
}

#[test]
fn test_decode_datetime() {
    let buf = [0u8; 8];
    let date: NaiveDateTime = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2000-01-01 00:00:00");

    let buf = 3_600_000_000i64.to_be_bytes();
    let date: NaiveDateTime = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2000-01-01 01:00:00");

    let buf = 629_377_265_000_000i64.to_be_bytes();
    let date: NaiveDateTime = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2019-12-11 11:01:05");
}

#[test]
fn test_encode_date() {
    let mut buf = Vec::new();

    let date = NaiveDate::from_ymd(2000, 1, 1);
    Encode::<Postgres>::encode(&date, &mut buf);
    assert_eq!(buf, [0; 4]);
    buf.clear();

    let date2 = NaiveDate::from_ymd(2001, 1, 1);
    Encode::<Postgres>::encode(&date2, &mut buf);
    // 2000 was a leap year
    assert_eq!(buf, 366i32.to_be_bytes());
    buf.clear();

    let date3 = NaiveDate::from_ymd(2019, 12, 11);
    Encode::<Postgres>::encode(&date3, &mut buf);
    assert_eq!(buf, 7284i32.to_be_bytes());
    buf.clear();
}

#[test]
fn test_decode_date() {
    let buf = [0; 4];
    let date: NaiveDate = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2000-01-01");

    let buf = 366i32.to_be_bytes();
    let date: NaiveDate = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2001-01-01");

    let buf = 7284i32.to_be_bytes();
    let date: NaiveDate = Decode::<Postgres>::decode(Some(&buf));
    assert_eq!(date.to_string(), "2019-12-11");
}