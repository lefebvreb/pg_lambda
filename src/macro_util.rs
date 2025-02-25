pub use std::borrow::Cow;
pub use std::io::Result;
pub use std::ops::Fn;
pub use std::vec::Vec;

pub use constcat::concat;
pub use inventory::submit;

pub use crate::lambda::PgLambda;
pub use crate::schema::{
    Column, ColumnTuple, Constraint, Name, ReferentialAction, Table, TableName, TableSchema,
};
pub use crate::types::{PgType, SetOf, ToPgValue};

pub struct TableDef {
    pub schema: &'static TableSchema,
}

inventory::collect!(TableDef);

pub struct PgLambdaDef {
    pub name: &'static str,
    pub create_statement: &'static str,
}

inventory::collect!(PgLambdaDef);

pub fn new_lambda<F, R>(statement: &'static str, write_params: F) -> PgLambda<F, R> {
    PgLambda::new(statement, write_params)
}

pub fn write_argcount(count: i32, dst: &mut Vec<u8>) {
    dst.extend(&count.to_be_bytes());
}

pub fn write_arg<T: PgType, U: ToPgValue<T>>(value: &U, dst: &mut Vec<u8>) -> Result<()> {
    if value.is_null() {
        dst.extend(&(-1i32).to_be_bytes());
    } else {
        let len = dst.len();
        dst.extend([0; 4]);
        U::write(value, dst)?;
        let size = (dst.len() - len) as i32;
        dst[len..len + 4].copy_from_slice(&size.to_be_bytes());
    }
    Ok(())
}
