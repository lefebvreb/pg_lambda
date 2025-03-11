use std::borrow::Cow;
use std::io::Result;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

pub use pg_lambda_macros::Table;

use crate::__proc_macro_util::TableDef;
use crate::migrations::Migrations;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Name<'a>(pub Cow<'a, str>);

impl<'a> Name<'a> {
    pub const fn new(name: &'a str) -> Self {
        Self(Cow::Borrowed(name))
    }

    fn to_sql(&self, f: &mut String) {
        f.push('"');
        f.push_str(&self.0);
        f.push('"');
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableName<'a> {
    pub schema: Option<Name<'a>>,
    pub name: Name<'a>,
}

impl TableName<'_> {
    fn to_sql(&self, f: &mut String) {
        if let Some(schema) = &self.schema {
            schema.to_sql(f);
            f.push('.');
        }
        self.name.to_sql(f);
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column<'a> {
    pub name: Name<'a>,
    pub ty: Cow<'static, str>,
}

impl Column<'_> {
    fn to_sql(&self, f: &mut String) {
        self.name.to_sql(f);
        f.push(' ');
        f.push_str(&self.ty);
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NameTuple<'a>(pub Cow<'a, [Name<'a>]>);

impl NameTuple<'_> {
    fn to_sql(&self, f: &mut String) {
        f.push('(');
        if let Some((last, head)) = self.0.split_last() {
            for name in head {
                name.to_sql(f);
                f.push(',');
            }
            last.to_sql(f);
        }
        f.push(')');
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReferentialAction<'a> {
    NoAction,
    Restrict,
    Cascade,
    SetNull { columns: NameTuple<'a> },
    SetDefault { columns: NameTuple<'a> },
}

impl ReferentialAction<'_> {
    fn to_sql(&self, f: &mut String) {
        match self {
            Self::NoAction => f.push_str("NO ACTION"),
            Self::Restrict => f.push_str("RESTRICT"),
            Self::Cascade => f.push_str("CASCADE"),
            Self::SetNull { columns } => {
                f.push_str("SET NULL ");
                columns.to_sql(f);
            }
            Self::SetDefault { columns } => {
                f.push_str("SET DEFAULT ");
                columns.to_sql(f);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Constraint<'a> {
    Check {
        expr: Cow<'a, str>,
    },
    Unique {
        columns: NameTuple<'a>,
        nulls_not_distinct: bool,
    },
    PrimaryKey {
        columns: NameTuple<'a>,
    },
    ForeignKey {
        columns: NameTuple<'a>,
        table: TableName<'a>,
        ref_columns: NameTuple<'a>,
        on_delete: ReferentialAction<'a>,
    },
}

impl Constraint<'_> {
    fn to_sql(&self, f: &mut String) {
        match self {
            Self::Check { expr } => {
                f.push_str("CHECK ");
                f.push_str(expr);
            }
            Self::Unique {
                columns,
                nulls_not_distinct,
            } => {
                f.push_str("UNIQUE ");
                if *nulls_not_distinct {
                    f.push_str("NULLS NOT DISTINCT ");
                }
                columns.to_sql(f);
            }
            Self::PrimaryKey { columns } => {
                f.push_str("PRIMARY KEY ");
                columns.to_sql(f);
            }
            Self::ForeignKey {
                columns,
                table,
                ref_columns: references,
                on_delete,
                ..
            } => {
                f.push_str("FOREIGN KEY ");
                columns.to_sql(f);
                f.push_str("REFERENCES ");
                table.to_sql(f);
                f.push(' ');
                references.to_sql(f);
                f.push_str(" ON DELETE ");
                on_delete.to_sql(f);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchema<'a> {
    pub name: TableName<'a>,
    pub columns: Cow<'a, [Column<'a>]>,
    pub constraints: Cow<'a, [Constraint<'a>]>,
}

/// Struct that corresponds to a table definition. Do not implement this trait manually.
pub trait Table {
    const SCHEMA: TableSchema<'static>;
}

pub(crate) enum SchemaOp<'a> {
    DropConstraint {
        table: TableName<'a>,
        constraint: Name<'a>,
    },
    DropColumn {
        table: TableName<'a>,
        column: Name<'a>,
    },
    DropTable {
        table: TableName<'a>,
    },
    DropSchema {
        schema: &'a str,
    },
    CreateSchema {
        schema: &'a str,
    },
    CreateTable {
        table: TableName<'a>,
    },
    AddColumn {
        table: TableName<'a>,
        column: Column<'a>,
    },
    AddConstraint {
        table: TableName<'a>,
        constraint: Constraint<'a>,
    },
}

impl SchemaOp<'_> {
    pub(crate) fn is_destructive(&self) -> bool {
        matches!(self, Self::DropTable { .. } | Self::DropColumn { .. })
    }

    pub(crate) fn to_sql(&self, f: &mut String) {
        match self {
            Self::DropConstraint {
                table,
                constraint: constraint_name,
            } => {
                f.push_str("ALTER TABLE ");
                table.to_sql(f);
                f.push_str(" DROP CONSTRAINT ");
                constraint_name.to_sql(f);
            }
            Self::DropColumn { table, column } => {
                f.push_str("ALTER TABLE ");
                table.to_sql(f);
                f.push_str(" DROP COLUMN ");
                column.to_sql(f);
            }
            Self::DropTable { table } => {
                f.push_str("DROP TABLE ");
                table.to_sql(f);
            }
            Self::DropSchema { schema } => {
                f.push_str("DROP SCHEMA ");
                f.push_str(schema);
            }
            Self::CreateSchema { schema } => {
                f.push_str("CREATE SCHEMA ");
                f.push_str(schema);
            }
            Self::CreateTable { table } => {
                f.push_str("CREATE TABLE ");
                table.to_sql(f);
                f.push_str(" ()");
            }
            Self::AddColumn { table, column } => {
                f.push_str("ALTER TABLE ");
                table.to_sql(f);
                f.push_str(" CREATE COLUMN ");
                column.to_sql(f);
            }
            Self::AddConstraint { table, constraint } => {
                f.push_str("ALTER TABLE ");
                table.to_sql(f);
                f.push_str(" ADD CONSTRAINT ");
                constraint.to_sql(f);
            }
        }
    }

    pub(crate) fn sort_key(&self) -> i32 {
        match self {
            Self::DropConstraint { .. } => 1,
            Self::DropColumn { .. } => 2,
            Self::DropTable { .. } => 3,
            Self::DropSchema { .. } => 4,
            Self::CreateSchema { .. } => 5,
            Self::CreateTable { .. } => 6,
            Self::AddColumn { .. } => 7,
            Self::AddConstraint { .. } => 8,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema<'a> {
    version: i32,
    tables: Cow<'a, [Cow<'a, TableSchema<'a>>]>,
}

lazy_static! {
    static ref SCHEMA: Schema<'static> = {
        let tables = inventory::iter::<TableDef>
            .into_iter()
            .map(|def| Cow::Borrowed(def.schema))
            .collect::<Vec<_>>();
        Schema::new(tables)
    };
}

impl<'a> Schema<'a> {
    pub const VERSION: i32 = 0;

    pub fn new(tables: impl Into<Cow<'a, [Cow<'a, TableSchema<'a>>]>>) -> Self {
        Self {
            version: Self::VERSION,
            tables: tables.into(),
        }
    }

    pub fn global() -> &'static Schema<'static> {
        &SCHEMA
    }

    pub fn tables(&self) -> &[Cow<'a, TableSchema<'a>>] {
        &self.tables
    }

    pub fn validate(&self) -> Result<()> {
        // forbid schema "__pg_lambda".
        // Sanitize identifiers.
        //
        todo!()
    }

    pub fn migrations(&'a self) -> Migrations<'a> {
        Migrations::new(self)
    }

    pub(crate) fn diff(&self, other: &Self) -> Vec<SchemaOp> {
        todo!()
    }
}
