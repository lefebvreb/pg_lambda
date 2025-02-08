use std::borrow::Cow;
use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};
use tokio_postgres::types::Type;

// mod driver;
mod types;

// macro_rules! plpgsql {
//     {
//         pub fn $name:ident ($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)? {
//             $($t:tt)*
//         }
//     } => {}
// }

// plpgsql! {
//     pub fn my_func(x: i32) -> i32 {
//         RETURN QUERY SELECT * FROM "employees" WHERE "age" >= "x";
//     }
// }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableName {
    pub schema: Option<Cow<'static, str>>,
    pub name: Cow<'static, str>,
}

impl Display for TableName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if let Some(schema) = &self.schema {
            write!(f, "\"{schema}\".")?;
        }
        write!(f, "\"{}\"", self.name)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnName(pub Cow<'static, str>);

impl Display for ColumnName {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "\"{}\"", self.0)
    }
}

#[derive(Clone)]
pub struct Column {
    pub name: ColumnName,
    pub ty: Type,
}

impl Display for Column {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} {}", self.name, self.ty)
    }
}

// #[derive(Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
// pub enum MatchKind {
//     Full,
//     Partial,
//     Simple,
// }

#[derive(Clone, Serialize, Deserialize)]
pub struct ColumnTuple {
    pub columns: Cow<'static, [ColumnName]>,
}

impl Display for ColumnTuple {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "(")?;
        if let Some((last, head)) = self.columns.split_last() {
            for column in head {
                write!(f, "{column},")?;
            }
            write!(f, "{last}")?;
        }
        write!(f, ")")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull { columns: ColumnTuple },
    SetDefault { columns: ColumnTuple },
}

// ALTER TABLE name ADD
// [ CONSTRAINT constraint_name ]
// { CHECK ( expression ) [ NO INHERIT ] |
//     UNIQUE [ NULLS [ NOT ] DISTINCT ] ( column_name [, ... ] ) index_parameters |
//     PRIMARY KEY ( column_name [, ... ] ) index_parameters |
//     EXCLUDE [ USING index_method ] ( exclude_element WITH operator [, ... ] ) index_parameters [ WHERE ( predicate ) ] |
//     FOREIGN KEY ( column_name [, ... ] ) REFERENCES reftable [ ( refcolumn [, ... ] ) ]
//       [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ] [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
//   [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ]

#[derive(Clone, Serialize, Deserialize)]
pub enum Constraint {
    Check {
        expr: Cow<'static, str>,
    },
    Unique {
        columns: ColumnTuple,
        nulls_not_distinct: bool,
    },
    PrimaryKey {
        columns: ColumnTuple,
    },
    ForeignKey {
        columns: ColumnTuple,
        table: TableName,
        references: ColumnTuple,
        on_delete: ReferentialAction,
    },
}

impl Constraint {
    pub fn name(&self) -> String {
        todo!()
    }
}

impl Display for Constraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Check { expr } => write!(f, "CHECK {expr}"),
            Self::Unique { columns, nulls_not_distinct } => {
                write!(f, "UNIQUE ")?;
                if *nulls_not_distinct {
                    write!(f, "NULLS NOT DISTINCT ")?;
                }
                write!(f, "{columns}")
            },
            Self::PrimaryKey { columns } => todo!(),
            Self::ForeignKey { columns, table, references, on_delete } => todo!(),
        }
    }
}

enum SchemaOp {
    CreateTable {
        table: TableName,
    },
    DropTable {
        table: TableName,
    },
    AddColumn {
        table: TableName,
        column: Column,
    },
    DropColumn {
        table: TableName,
        column: ColumnName,
    },
    AddConstraint {
        table: TableName,
        constraint: Constraint,
    },
    DropConstraint {
        table: TableName,
        constraint: &'static str,
    },
}

impl SchemaOp {
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::DropTable { .. } | Self::DropColumn { .. })
    }
}

impl Display for SchemaOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            SchemaOp::CreateTable { table } => write!(f, "CREATE TABLE {table};"),
            SchemaOp::DropTable { table } => write!(f, "DROP TABLE {table};"),
            SchemaOp::AddColumn { table, column } => write!(f, "ALTER TABLE {table} ADD COLUMN {column};"),
            SchemaOp::DropColumn { table, column } => write!(f, "ALTER TABLE {table} DROP COLUMN {column};"),
            SchemaOp::AddConstraint { table, constraint } => write!(f, "ALTER TABLE {table} ADD CONSTRAINT {} {constraint};", constraint.name()),
            SchemaOp::DropConstraint { table, constraint } => write!(f, "ALTER TABLE {table} DROP CONSTRAINT {constraint};"),
        }
    }
}

fn main() {}
