use std::borrow::Cow;
use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};
use tokio_postgres::types::Type;

// mod driver;

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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MatchKind {
    Full,
    Partial,
    Simple,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull { columns: Cow<'static, [ColumnName]> },
    SetDefault { columns: Cow<'static, [ColumnName]> },
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

#[derive(Clone)]
pub enum Constraint {
    Check {
        constraint_name: Cow<'static, str>,
        expr: Cow<'static, str>,
    },
    Unique {
        columns: Cow<'static, [ColumnName]>,
        nulls_not_distinct: bool,
    },
    PrimaryKey {
        columns: Cow<'static, [ColumnName]>,
    },
    ForeignKey {
        columns: Cow<'static, [ColumnName]>,
        table: TableName,
        references: Cow<'static, [ColumnName]>,
        match_kind: MatchKind,
        on_delete: ReferentialAction,
        on_update: ReferentialAction,
    },
}

impl Constraint {
    pub fn name(&self) -> String {
        todo!()
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
        column: ColumnName,
    },
    DropConstraint {
        constraint: &'static str,
    },
}

impl SchemaOp {
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::DropTable { .. } | Self::DropColumn { .. })
    }

    pub fn to_query(&self) -> String {
        match self {
            SchemaOp::CreateTable { table } => format!("CREATE TABLE {table};"),
            SchemaOp::DropTable { table } => format!("DROP TABLE {table};"),
            SchemaOp::AddColumn { table, column } => {
                format!("ALTER TABLE {table} ADD COLUMN {column};")
            }
            SchemaOp::DropColumn { table, column } => {
                format!("ALTER TABLE {table} DROP COLUMN {column};")
            }
            SchemaOp::AddConstraint { table, column } => todo!(),
            SchemaOp::DropConstraint {
                constraint: constraint_name,
            } => todo!(),
        }
    }
}

fn main() {}
