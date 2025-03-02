# PgLambda

## TODO

Before `0.1.0`:
* Allow nullable types, where applicable.
* Allow composite types like arrays and tuples, where applicable.
* Make schema migrations runner.

After `0.1.0`:
* Parse `Config` structs from PostgreSQL connection strings or PostgreSQL URIs.
* Make a derive for custom `FromRow`s.
* Add support for more PostgreSQL types.
* Add support for SSL stream encryption with `rustls` and `tokio-rustls`.
* Add more features to schemas.
* Support more authentication methods.
