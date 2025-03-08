use std::net::TcpStream;

use pg_lambda::connection::Connection;
use pg_lambda::connection::params::NoParams;
use pg_lambda::connection::result::{SetOf, Void};
use pg_lambda::types::{Integer, Text};

mod util;

#[test]
fn transaction() {
    util::with_test_database(async |config| {
        let mut conn1 = Connection::<TcpStream>::connect_sync(&config)?;
        let mut conn2 = Connection::<TcpStream>::connect_sync(&config)?;

        conn1.query_sync::<Void, ()>("CREATE TABLE persons (name TEXT, age INT)", NoParams)?;

        let mut trans = conn1.transaction_sync()?;
        trans.query_sync::<Void, ()>(
            "INSERT INTO persons (name, age) VALUES ('john', 42)",
            NoParams,
        )?;

        let res = conn2.query_sync::<SetOf<(Text, Integer)>, Vec<(String, i32)>>(
            "SELECT name, age FROM persons",
            NoParams,
        )?;
        assert_eq!(res, vec![]);

        trans.commit_sync()?;

        let res = conn2.query_sync::<SetOf<(Text, Integer)>, Vec<(String, i32)>>(
            "SELECT name, age FROM persons",
            NoParams,
        )?;
        assert_eq!(res, vec![("john".to_string(), 42)]);

        Ok(())
    });
}
