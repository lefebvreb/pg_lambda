use std::borrow::Cow;
use std::fmt::{Display, Formatter, Result as FmtResult};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

pub use pg_lambda_macros::Table;

use crate::__proc_macro_util::TableDef;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Name(pub Cow<'static, str>);

impl Name {
    pub const fn new(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "\"{}\"", self.0)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableName {
    pub schema: Option<Name>,
    pub name: Name,
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
pub struct Column {
    pub name: Name,
    pub ty: Cow<'static, str>,
}

impl Display for Column {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} {}", self.name, self.ty)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColumnTuple(pub Cow<'static, [Name]>);

impl Display for ColumnTuple {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "(")?;
        if let Some((last, head)) = self.0.split_last() {
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

impl Display for ReferentialAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::NoAction => write!(f, "NO ACTION"),
            Self::Restrict => write!(f, "RESTRICT"),
            Self::Cascade => write!(f, "CASCADE"),
            Self::SetNull { columns } => write!(f, "SET NULL {columns}"),
            Self::SetDefault { columns } => write!(f, "SET DEFAULT {columns}"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
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
        ref_columns: ColumnTuple,
        on_delete: ReferentialAction,
    },
}

impl Display for Constraint {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Check { expr } => write!(f, "CHECK {expr}"),
            Self::Unique {
                columns,
                nulls_not_distinct,
            } => {
                write!(f, "UNIQUE ")?;
                if *nulls_not_distinct {
                    write!(f, "NULLS NOT DISTINCT ")?;
                }
                write!(f, "{columns}")
            }
            Self::PrimaryKey { columns } => write!(f, "PRIMARY KEY {columns}"),
            Self::ForeignKey {
                columns,
                table,
                ref_columns: references,
                on_delete,
                ..
            } => write!(
                f,
                "FOREIGN KEY {columns} REFERENCES {table} {references} ON DELETE {on_delete}"
            ),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SchemaOp {
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
        column: Name,
    },
    AddConstraint {
        table: TableName,
        constraint: Constraint,
    },
    DropConstraint {
        table: TableName,
        constraint_name: &'static str,
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
            Self::CreateTable { table } => write!(f, "CREATE TABLE {table};"),
            Self::DropTable { table } => write!(f, "DROP TABLE {table};"),
            Self::AddColumn { table, column } => {
                write!(f, "ALTER TABLE {table} ADD COLUMN {column};")
            }
            Self::DropColumn { table, column } => {
                write!(f, "ALTER TABLE {table} DROP COLUMN {column};")
            }
            Self::AddConstraint { table, constraint } => {
                write!(f, "ALTER TABLE {table} ADD CONSTRAINT {constraint};")
            }
            Self::DropConstraint {
                table,
                constraint_name,
            } => {
                write!(
                    f,
                    "ALTER TABLE {table} DROP CONSTRAINT \"{constraint_name}\";"
                )
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchema {
    pub name: TableName,
    pub columns: Cow<'static, [Column]>,
    pub constraints: Cow<'static, [Constraint]>,
}

/// Struct that corresponds to a table definition. Do not implement this trait manually.
pub trait Table {
    const SCHEMA: TableSchema;
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub tables: Box<[Cow<'static, TableSchema>]>,
}

lazy_static! {
    static ref SCHEMA: Schema = Schema {
        tables: inventory::iter::<TableDef>
            .into_iter()
            .map(|def| Cow::Borrowed(def.schema))
            .collect(),
    };
}

impl Schema {
    pub fn get() -> &'static Self {
        &SCHEMA
    }

    fn diff(&self, other: &Self) -> Box<[SchemaOp]> {
        todo!()
    }
}
