use std::env::var;
use std::io::Result;
use std::net::TcpStream;
use std::panic::{UnwindSafe, catch_unwind, resume_unwind};

use futures::executor::block_on;
use pg_lambda::connection::params::NoParams;
use pg_lambda::connection::result::Void;
use pg_lambda::connection::{Config, Connection};

pub fn with_test_database(f: impl AsyncFnOnce(&Config) -> Result<()> + UnwindSafe) {
    // Retrieve config.
    // todo: use a single var with a connection string inside instead.
    let config = Config {
        user: var("PG_USER").unwrap_or_else(|_| "postgres".to_owned()),
        password: var("PG_PASSWORD").unwrap_or_else(|_| "postgres".to_owned()),
        database: var("PG_DATABASE").unwrap_or_else(|_| "postgres".to_owned()),
        host: var("PG_LOCALHOST").unwrap_or_else(|_| "localhost".to_owned()),
        port: var("PG_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .unwrap_or(5432),
    };

    // Open connection to master db.
    let res = Connection::<TcpStream>::connect_sync(&config);
    assert!(
        res.is_ok(),
        "failed to open connection to master database: {}",
        res.unwrap_err()
    );
    let mut conn = res.unwrap();

    // Create test database.
    let test_database = format!("testdb_{}", fastrand::u128(..));
    let res = conn.query_sync::<Void, ()>(
        &format!("CREATE DATABASE \"{test_database}\" TEMPLATE \"template0\""),
        NoParams,
    );
    assert!(
        res.is_ok(),
        "failed to create test database: {}",
        res.unwrap_err(),
    );

    // Close connection to master db.
    drop(conn);

    // Do test.
    let test_config = Config {
        database: test_database.clone(),
        ..config.clone()
    };
    // todo: add a timeout so it doesn't hang on forever.
    let test_res = catch_unwind(|| block_on(f(&test_config)));

    // Open connection to master db.
    let res = Connection::<TcpStream>::connect_sync(&config);
    assert!(
        res.is_ok(),
        "failed to open connection to master database: {}",
        res.unwrap_err()
    );
    let mut conn = res.unwrap();

    // Drop test database.
    let res = conn.query_sync::<Void, ()>(
        &format!("DROP DATABASE \"{test_database}\" (FORCE)"),
        NoParams,
    );
    assert!(
        res.is_ok(),
        "failed to drop test database \"{test_database}\": {}",
        res.unwrap_err(),
    );

    // Get test results.
    match test_res {
        Ok(res) => {
            assert!(res.is_ok(), "test failed with error: {}", res.unwrap_err());
        }
        Err(payload) => resume_unwind(payload),
    };
}
