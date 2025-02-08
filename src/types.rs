use tokio_postgres::types::{FromSql, ToSql, Type};

pub trait PgType {
    const SQL_NAME: &str;

    fn oid() -> Type;
}

pub trait FromPg<'a, T: PgType>: FromSql<'a> {}

pub trait IntoPg<T: PgType>: ToSql {}

impl<T, U> IntoPg<U> for &T
where
    T: IntoPg<U>,
    U: PgType,
{}

impl<'a, T, U> FromPg<'a, U> for T
where
    T: IntoPg<U> + FromSql<'a>,
    U: PgType,
{}

pub struct Int4;

impl PgType for Int4 {
    const SQL_NAME: &str = "INT4";

    fn oid() -> Type {
        Type::INT4
    }
}

impl IntoPg<Int4> for i32 {}
