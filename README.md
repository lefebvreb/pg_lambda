# PgLambda

## TODO

Before `0.1.0`:
* Allow nullable types, where applicable.
* Allow composite types like arrays and tuples, where applicable.
* Make schema migrations runner.
* We no longer need sha1 for functions, use a simple UUID instead.

After `0.1.0`:
* Parse `Config` structs from PostgreSQL connection strings or PostgreSQL URIs.
* Make a derive for custom `FromRow`s.
* Support for more PostgreSQL types.
* Support for SSL stream encryption with `rustls` and `tokio-rustls`.
* Add more features to schemas.
* Support more authentication methods.
* Add timeout on transport read/writes.
* Support custom migrations.
* Improve connection life-cyle, with recycling and transactions handling
* Support indexes in schema.
* Have default types for results.
* Improve error reporting.
* Add support for [plpgsql_check](https://github.com/okbob/plpgsql_check) to check lambdas.
