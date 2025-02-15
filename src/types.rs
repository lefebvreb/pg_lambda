use tokio_postgres::types::{FromSql, ToSql, Type};

pub trait PgType {
    const SQL_NAME: &str;

    fn oid() -> Type;
}

pub trait FromPg<'a, T: PgType>: FromSql<'a> {}

pub trait ToPg<T: PgType>: ToSql {}

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
                impl<'a> FromPg<'a, $name> for $from {}
            )*

            $(
                impl ToPg<$name> for $to {}
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
