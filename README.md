# PgLambda

## TODO

Before `0.1.0`:
* Make a macro to generate write_params implementations
* Make a stream for SetOf query results
* Allow nullable types, where applicable
* Allow composite types like arrays and tuples, where applicable
* Make schema migrations runner

After `0.1.0`:
* Parse `Config` structs from PostgreSQL connection strings or PostgreSQL URIs.
* Add support for more PostgreSQL types.
* Add support for SSL stream encryption with `rustls` and `tokio-rustls`.
* Add more features to schemas.
