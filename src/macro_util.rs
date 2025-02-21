pub use std::borrow::Cow;
pub use std::io::Result;
pub use std::ops::Fn;
pub use std::vec::Vec;
use std::marker::PhantomData;

pub use constcat::concat;
pub use inventory::submit;

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

pub fn new_lambda<Args, Output>(statement: &'static str, args: Args) -> PgLambda<Args, Output> {
    PgLambda {
        statement,
        args,
        _marker: PhantomData,
    }
}

pub fn write_sized<T: PgType, U: ToPgValue<T>>(value: &U, dst: &mut Vec<u8>) -> Result<()> {
    let len = dst.len();
    dst.extend([0; 4]);
    U::write(value, dst)?;
    let size = dst.len() - len;
    dst[len..len+4].copy_from_slice(&size.to_be_bytes());
    Ok(())
}
