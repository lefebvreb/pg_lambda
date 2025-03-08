use std::net::TcpStream;

use pg_lambda::connection::Connection;
use pg_lambda::connection::params::NoParams;
use pg_lambda::connection::result::{Single, Void};
use pg_lambda::types::Integer;
use pg_lambda_macros::query_params;

mod util;

#[test]
fn empty_query() {
    util::with_test_database(async |config| {
        let mut conn = Connection::<TcpStream>::connect_sync(&config)?;
        conn.query_sync::<Void, ()>("", NoParams)?;
        Ok(())
    });
}

#[test]
fn echo_query() {
    util::with_test_database(async |config| {
        let mut conn = Connection::<TcpStream>::connect_sync(&config)?;
        let n = conn
            .query_sync::<Single<Integer>, i32>("SELECT $1 * 2", query_params!(Integer(&42i32)))?;
        assert_eq!(n, 84);
        Ok(())
    });
}
