use std::io::Result;

use futures::FutureExt;

use crate::__proc_macro_util::{Single, Void};
use crate::connection::params::NoParams;
use crate::connection::{Connection, SyncTransport, Transport};
use crate::schema::Schema;
use crate::types::Boolean;

mod queries {
    pub const SCHEMA_EXISTS: &str = r#"SELECT EXISTS (SELECT FROM "information_schema"."schemata" WHERE "schema_name" = '__pg_lambda')"#;
    pub const CREATE_SCHEMA: &str = r#"CREATE SCHEMA "__pg_lambda""#;
    pub const CREATE_TABLE_SCHEMAS: &str = r#"CREATE TABLE "__pg_lambda"."schemas" ("run_at" TIMESTAMPZ NOT NULL DEFAULT NOW(), "schema" JSONB NOT NULL)"#;
    pub const CREATE_TABLE_LAMBDAS: &str =
        r#"CREATE TABLE "__pg_lambda"."lambdas" ("name" TEXT NOT NULL UNIQUE)"#;
}

pub struct Migrations<'a> {
    schema: &'a Schema<'a>,
    allow_destructive: bool,
}

impl<'a> Migrations<'a> {
    pub(crate) fn new(schema: &'a Schema<'a>) -> Self {
        Self {
            schema,
            allow_destructive: false,
        }
    }

    pub fn allow_destructive(&mut self, allow_destructive: bool) -> &mut Self {
        self.allow_destructive = allow_destructive;
        self
    }

    pub async fn run<T>(&mut self, conn: &mut Connection<T>) -> Result<()>
    where
        T: Transport,
    {
        // 1. Open transaction
        // 2. Get current schema from database:
        //   a. If there is a schema, use it
        //   b. If there no schema, create the necessary scaffolding
        // 3. Compute diff with our schema
        // 4. Execute all diff instructions
        // 5. Commit transaction in case of success, else abort

        let mut tr = conn.transaction().await?;

        // Check if private schema is already there.
        let initialized = tr
            .query::<Single<Boolean>, bool>(queries::SCHEMA_EXISTS, NoParams)
            .await?;

        // Create "__pg_lambda" schema if it does not exist alreay.
        if !initialized {
            tr.query::<Void, ()>(queries::CREATE_SCHEMA, NoParams)
                .await?;
            tr.query::<Void, ()>(queries::CREATE_TABLE_SCHEMAS, NoParams)
                .await?;
            tr.query::<Void, ()>(queries::CREATE_TABLE_LAMBDAS, NoParams)
                .await?;
        }

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
