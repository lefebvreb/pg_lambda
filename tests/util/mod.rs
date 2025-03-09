use std::env::var;
use std::io::{Error, ErrorKind, Result};
use std::net::TcpStream;
use std::panic::{UnwindSafe, catch_unwind, resume_unwind};
use std::sync::mpsc::channel;
use std::thread::spawn;
use std::time::Duration;

use pg_lambda::connection::params::NoParams;
use pg_lambda::connection::result::Void;
use pg_lambda::connection::{Config, Connection};
use tokio::runtime::Runtime;

const TIMEOUT: Duration = Duration::from_secs(5);

pub fn with_test_database(
    f: impl AsyncFnOnce(&Config) -> Result<()> + UnwindSafe + Send + 'static,
) {
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

    // Create test database.
    let test_database = {
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

        test_database
    };

    // Do the test.
    let test_res = {
        // Crate test config.
        let test_config = Config {
            database: test_database.clone(),
            ..config.clone()
        };

        // Open a channel to receive results.
        let (tx, rx) = channel();

        // Spawn the thread the test will be carried on.
        spawn(move || {
            // Catch any panics during the test.
            let res = catch_unwind(|| {
                let rt = Runtime::new()?;
                rt.block_on(f(&test_config))
            });

            // Send results through the channel. This might fail if the test already timed out.
            tx.send(res).ok();
        });

        // Receive the results, with a timeout.
        match rx.recv_timeout(TIMEOUT) {
            Ok(res) => res,
            Err(_) => Ok(Err(Error::new(ErrorKind::TimedOut, "test timed out"))),
        }
    };

    // Drop the test database.
    {
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
    }

    // Examine test results.
    match test_res {
        Ok(res) => {
            assert!(res.is_ok(), "test failed with error: {}", res.unwrap_err());
        }
        Err(payload) => resume_unwind(payload),
    };
}
