pub use std::borrow::Cow;
pub use std::ops::Fn;
pub use std::vec::Vec;

pub use constcat::concat;
pub use inventory::submit;

pub use crate::connection::params::QueryParams;
pub use crate::connection::result::{SetOf, Single, Void};
pub use crate::lambda::PgLambda;
pub use crate::schema::{
    Column, ColumnTuple, Constraint, Name, ReferentialAction, Table, TableName, TableSchema,
};
pub use crate::types::{PgType, ToPgValue};

pub struct TableDef {
    pub schema: &'static TableSchema,
}

inventory::collect!(TableDef);

pub struct PgLambdaDef {
    pub name: &'static str,
    pub create_statement: &'static str,
}

inventory::collect!(PgLambdaDef);

pub fn new_params_fn<F>(f: F) -> crate::connection::params::ParamsFn<F>
where
    F: FnOnce(&mut Vec<u8>) -> std::io::Result<()>,
{
    crate::connection::params::ParamsFn::new(f)
}

pub fn new_lambda<P, R>(statement: &'static str, params: P) -> PgLambda<P, R>
where
    P: QueryParams,
{
    PgLambda::new(statement, params)
}

pub fn write_i32(count: i32, dst: &mut Vec<u8>) {
    dst.extend(&count.to_be_bytes());
}

pub fn write_value<T: PgType, U: ToPgValue<T>>(
    value: &U,
    dst: &mut Vec<u8>,
) -> std::io::Result<()> {
    if value.is_null() {
        write_i32(-1, dst);
    } else {
        let len = dst.len();
        dst.extend([0; 4]);
        U::write(value, dst)?;
        let size = (dst.len() - len) as i32;
        dst[len..len + 4].copy_from_slice(&size.to_be_bytes());
    }
    Ok(())
}
