use std::io::Result;

use futures::FutureExt;

use crate::connection::{Connection, SyncTransport, Transport};
use crate::schema::Schema;

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
        // 5. Drop all old lambdas
        // 6. Create all new lambdas
        // 7. Commit transaction in case of success, else abort
        todo!()
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
