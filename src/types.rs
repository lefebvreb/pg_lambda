use tokio_postgres::types::{FromSql, ToSql, Type};
use tokio_postgres::Row;

use crate::lambda::PgLambdaError;

pub trait PgType {
    const SQL_NAME: &str;
    fn oid() -> Type;
}

// TODO: probably can remove these Send and Sync when we get rid of tokio-postgres
pub trait ToPgValue<T: PgType>: ToSql + Send + Sync {}

pub trait FromPgValue<'a, T: PgType>: FromSql<'a> {}

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
    Void {
        sql_name: "VOID",
        oid: Type::VOID,
        // TODO: switch to () when we are no longer constrained by tokio-postgres
        from: [Void],
        to: [Void],
    },
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

// TODO: get rid of this
const _: () = {
    impl<'a> FromSql<'a> for Void {
        fn from_sql(
            _: &Type,
            _: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            Ok(Self)
        }

        fn accepts(ty: &Type) -> bool {
            ty == &Type::VOID
        }
    }

    impl ToSql for Void {
        fn to_sql(
            &self,
            _: &Type,
            _: &mut tokio_postgres::types::private::BytesMut,
        ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
        {
            Ok(tokio_postgres::types::IsNull::No)
        }

        fn accepts(ty: &Type) -> bool {
            ty == &Type::VOID
        }

        fn to_sql_checked(
            &self,
            ty: &Type,
            out: &mut tokio_postgres::types::private::BytesMut,
        ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
        {
            if !<Self as ToSql>::accepts(ty) {
                return Err(Box::new(tokio_postgres::types::WrongType::new::<Self>(
                    ty.clone(),
                )));
            }
            self.to_sql(ty, out)
        }
    }
};

pub trait FromQueryResult<'a, T>: Sized {
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError>;
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError> {
        Ok(rows[0].get(0))
    }
}

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromQueryResult<'a, (T0,)> for Vec<(U0,)> {
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError> {
        Ok(rows.iter().map(|row| (row.get(0),)).collect())
    }
}

impl<'a, T0: PgType, T1: PgType, U0: FromPgValue<'a, T0>, U1: FromPgValue<'a, T1>>
    FromQueryResult<'a, (T0, T1)> for Vec<(U0, U1)>
{
    fn from_rows(rows: &'a [Row]) -> Result<Self, PgLambdaError> {
        Ok(rows.iter().map(|row| (row.get(0), row.get(1))).collect())
    }
}

// etc.
