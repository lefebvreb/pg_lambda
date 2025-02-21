use std::any::Any;
pub use std::borrow::Cow;
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

pub fn create_pg_lambda<'a, T>(
    statement: &'static str,
    params: Box<[&'a dyn Any]>,
) -> PgLambda<'a, T> {
    PgLambda {
        statement,
        // params,
        _marker: PhantomData,
    }
}
