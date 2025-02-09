use std::net::IpAddr;
use std::time::SystemTime;

use tokio_postgres::types::{FromSql, ToSql, Type};

pub trait PgType<'a>: FromSql<'a> + ToSql {
    const SQL_NAME: &'static str;
    const NOT_NULL: bool = true;

    fn oid() -> Type;
}

impl<'a> PgType<'a> for &'a str {
    const SQL_NAME: &'static str = "TEXT";

    fn oid() -> Type {
        Type::TEXT
    }
}

impl<'a> PgType<'a> for &'a [u8] {
    const SQL_NAME: &'static str = "BYTEA";

    fn oid() -> Type {
        Type::BYTEA
    }
}

impl PgType<'_> for IpAddr {
    const SQL_NAME: &'static str = "INET";

    fn oid() -> Type {
        Type::INET
    }
}

impl PgType<'_> for bool {
    const SQL_NAME: &'static str = "BOOL";

    fn oid() -> Type {
        Type::BOOL
    }
}

impl PgType<'_> for f32 {
    const SQL_NAME: &'static str = "FLOAT4";

    fn oid() -> Type {
        Type::FLOAT4
    }
}

impl PgType<'_> for f64 {
    const SQL_NAME: &'static str = "FLOAT8";

    fn oid() -> Type {
        Type::FLOAT4
    }
}

impl PgType<'_> for i8 {
    const SQL_NAME: &'static str = "CHAR";

    fn oid() -> Type {
        Type::CHAR
    }
}

impl PgType<'_> for i16 {
    const SQL_NAME: &'static str = "INT2";

    fn oid() -> Type {
        Type::INT2
    }
}

impl PgType<'_> for i32 {
    const SQL_NAME: &'static str = "INT4";

    fn oid() -> Type {
        Type::INT4
    }
}

impl PgType<'_> for i64 {
    const SQL_NAME: &'static str = "INT8";

    fn oid() -> Type {
        Type::INT8
    }
}

impl PgType<'_> for Box<str> {
    const SQL_NAME: &'static str = "TEXT";

    fn oid() -> Type {
        Type::TEXT
    }
}

impl PgType<'_> for String {
    const SQL_NAME: &'static str = "TEXT";

    fn oid() -> Type {
        Type::TEXT
    }
}

impl PgType<'_> for Vec<u8> {
    const SQL_NAME: &'static str = "BYTEA";

    fn oid() -> Type {
        Type::BYTEA
    }
}

impl PgType<'_> for SystemTime {
    const SQL_NAME: &'static str = "TIMESTAMP";

    fn oid() -> Type {
        Type::TIMESTAMP
    }
}

// impl<S: Default + BuildHasher> PgType<'_> for HashMap<String, Option<String>, S> {
//     const SQL_NAME: &'static str = "HSTORE";

//     fn oid() -> Type {
//         Type::???
//     }
// }

impl<'a, T: PgType<'a>> PgType<'a> for Option<T> {
    const SQL_NAME: &'static str = T::SQL_NAME;
    const NOT_NULL: bool = false;

    fn oid() -> Type {
        T::oid()
    }
}
