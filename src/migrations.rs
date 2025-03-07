use std::io::Result;

use futures::FutureExt;

use crate::connection::{Connection, SyncConnection, SyncTransport, Transport};
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
        todo!()
    }

    pub fn run_sync<T>(&mut self, conn: &mut SyncConnection<T>) -> Result<()>
    where
        T: SyncTransport,
    {
        self.run(conn)
            .now_or_never()
            .expect("future should resolve immediately")
    }
}
