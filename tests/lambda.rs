use pg_lambda::pg_lambda;

pg_lambda! {
    pub fn double(x: Integer) -> Integer r#"
        RETURN QUERY SELECT "x" * 2;
    "#

    pub fn square(x: Integer) -> Integer r#"
        RETURN QUERY SELECT "x" * "x;
    "#
}

#[test]
fn lambda() {}
