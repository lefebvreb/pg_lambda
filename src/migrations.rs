use std::io::{Error, ErrorKind, Result};

use futures::FutureExt;
use pg_lambda_macros::query_params;
use serde_json::value::RawValue;

use crate::__proc_macro_util::PgLambdaDef;
use crate::connection::params::NoParams;
use crate::connection::result::{SetOf, Single, Void};
use crate::connection::{Connection, SyncTransport, Transport};
use crate::schema::{Name, Schema, SchemaOp};
use crate::types::{Boolean, Jsonb, Text};

mod queries {
    pub const SCHEMA_EXISTS: &str = r#"
        SELECT EXISTS (
            SELECT FROM "information_schema"."schemata" 
            WHERE "schema_name" = '__pg_lambda'
        )
    "#;

    pub const CREATE_SCHEMA: &str = r#"
        CREATE SCHEMA "__pg_lambda"
    "#;

    pub const CREATE_SCHEMAS_TABLE: &str = r#"
        CREATE TABLE "__pg_lambda"."schemas" (
            "migration_date" TIMESTAMPZ NOT NULL DEFAULT NOW(),
            "schema" JSONB NOT NULL
        )
    "#;

    pub const CREATE_LAMBDAS_TABLE: &str = r#"
        CREATE TABLE "__pg_lambda"."lambdas" (
            "name" TEXT NOT NULL UNIQUE
        )
    "#;

    pub const SELECT_PREVIOUS_SCHEMA: &str = r#"
        SELECT "schema" FROM "__pg_lambda"."schemas" 
        ORDER BY "migration_date" DESC 
        LIMIT 1
    "#;

    pub const INSERT_NEW_SCHEMA: &str = r#"
        INSERT INTO "__pg_lambda"."schemas" ("schema") 
        VALUES ($1)
    "#;

    pub const DELETE_ALL_LAMBDAS_NAMES: &str = r#"
        DELETE FROM "__pg_lambda"."lambdas"
        RETURNING "name"
    "#;

    pub const INSERT_LAMBDAS_NAMES: &str = r#"
        INSERT INTO "__pg_lambda"."lambdas" ("name")
        SELECT UNNEST($1)
    "#;
}

pub struct Migrations<'a> {
    schema: &'a Schema<'a>,
    allow_destructive: bool,
    initialize_lambdas: bool,
}

impl<'a> Migrations<'a> {
    pub(crate) fn new(schema: &'a Schema<'a>) -> Self {
        Self {
            schema,
            allow_destructive: false,
            initialize_lambdas: true,
        }
    }

    pub fn allow_destructive(&mut self, allow_destructive: bool) -> &mut Self {
        self.allow_destructive = allow_destructive;
        self
    }

    pub fn initialize_lambdas(&mut self, initialize_lambdas: bool) -> &mut Self {
        self.initialize_lambdas = initialize_lambdas;
        self
    }

    pub async fn run<T>(&mut self, conn: &mut Connection<T>) -> Result<()>
    where
        T: Transport,
    {
        // Validate the schema.
        self.schema.validate()?;

        // Open transaction.
        let mut tr = conn.transaction().await?;

        // Check if private schema is already there.
        let initialized = tr
            .query::<Single<Boolean>, bool>(queries::SCHEMA_EXISTS, NoParams)
            .await?;

        // Create "__pg_lambda" schema if it does not exist alreay.
        if !initialized {
            tr.query::<Void, ()>(queries::CREATE_SCHEMA, NoParams)
                .await?;
            tr.query::<Void, ()>(queries::CREATE_SCHEMAS_TABLE, NoParams)
                .await?;
            tr.query::<Void, ()>(queries::CREATE_LAMBDAS_TABLE, NoParams)
                .await?;
        }

        // Fetch previous schema from the database.
        let json = tr
            .query::<SetOf<Jsonb>, Option<Box<RawValue>>>(queries::SELECT_PREVIOUS_SCHEMA, NoParams)
            .await?;

        // Parse schema, or use the default one if there was none before.
        let previous_schema = match json {
            Some(json) => serde_json::from_str(json.get())?,
            None => Schema::default(),
        };

        // Compute the diff between these two schemas.
        let diff = self.schema.diff(&previous_schema);

        // Check that the migration wouldn't lose data if the user requested that there be no data loss.
        if !self.allow_destructive && diff.iter().any(SchemaOp::is_destructive) {
            tr.rollback().await?;
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "migration requires destructive operations",
            ));
        }

        let mut statement = String::new();

        // Run all schema operations.
        for op in diff {
            op.to_sql(&mut statement);
            tr.query::<Void, ()>(&statement, NoParams).await?;
            statement.clear();
        }

        // Insert new schema in database.
        tr.query::<Void, ()>(
            queries::INSERT_NEW_SCHEMA,
            query_params!(Jsonb(self.schema)),
        )
        .await?;

        if self.initialize_lambdas {
            // Drop all previous lambdas.
            if initialized {
                // Get their names.
                let names = tr
                    .query::<SetOf<Text>, Vec<&str>>(queries::DELETE_ALL_LAMBDAS_NAMES, NoParams)
                    .await?;

                if let Some((last, head)) = names.split_last() {
                    // Prepare drop statement.
                    statement.push_str("DROP FUNCTION ");
                    for name in head {
                        Name::new(name).to_sql(&mut statement);
                        statement.push(',');
                    }
                    Name::new(last).to_sql(&mut statement);

                    // Execute drop statement.
                    tr.query::<Void, ()>(&statement, NoParams).await?;
                }
            }

            let mut names = Vec::new();

            // Create all lambdas.
            for def in inventory::iter::<PgLambdaDef>() {
                tr.query::<Void, ()>(def.create_statement, NoParams).await?;
                names.push(def.name);
            }

            // Insert them into the table.
            tr.query::<Void, ()>(queries::INSERT_LAMBDAS_NAMES, NoParams)
                .await?;
        }

        // Commit changes.
        tr.commit().await?;
        Ok(())
    }

    pub fn run_sync<T>(&mut self, conn: &mut Connection<T>) -> Result<()>
    where
        T: SyncTransport,
    {
        self.run(conn)
            .now_or_never()
            .expect("future should resolve immediately")
    }
}
