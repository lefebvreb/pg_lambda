use tokio_postgres::types::{FromSql, ToSql, Type};
use tokio_postgres::Row;

use crate::error::PgLambdaError;

pub trait PgType {
    const SQL_NAME: &str;
    fn oid() -> Type;
}

// TODO: probably can remove these Send and Sync when we get rid of tokio-postgres
pub trait ToPgValue<T: PgType>: ToSql + Send + Sync {}

pub trait FromPgValue<'a, T: PgType>: FromSql<'a> {}

pub trait FromQueryResult<'a, T: PgType>: FromSql<'a> {
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError>;
}

macro_rules! typedefs {
    (
        $(
            $name:ident {
                sql_name: $sql_name:literal,
                oid: $oid:expr,
                from: [$($from:ty),*],
                to: [$($to:ty),*],
                $(aliases: [$($alias:ident),*],)?
            },
        )*
    ) => {
        $(
            #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
            pub struct $name;
            impl PgType for $name {
                const SQL_NAME: &str = $sql_name;
                fn oid() -> Type { $oid }
            }

            $(
                impl<'a> FromPgValue<'a, $name> for $from {}
            )*

            $(
                impl ToPgValue<$name> for $to {}
            )*

            $(
                $(pub type $alias = $name;)*
            )?
        )*
    };
}

typedefs! {
    Int4 {
        sql_name: "INT4",
        oid: Type::INT4,
        from: [i32],
        to: [i32],
        aliases: [Integer, Int],
    },
    Text {
        sql_name: "TEXT",
        oid: Type::TEXT,
        from: [String, &'a str],
        to: [String],
    },
}

impl PgType for () {
    const SQL_NAME: &str = "VOID";
    fn oid() -> Type {
        Type::VOID
    }
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError> {
        Ok(rows[0].get(0))
    }
}
