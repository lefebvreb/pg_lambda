pub enum PgLambdaError {
    Protocol(tokio_postgres::Error),
}
