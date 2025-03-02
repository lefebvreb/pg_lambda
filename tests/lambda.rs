use pg_lambda::lambda::pg_lambda;
use pg_lambda::types::Integer;

pg_lambda! {
    pub fn double(x: Integer) -> Integer r#"
        RETURN QUERY SELECT "x" * 2;
    "#
}

#[test]
fn lambda() {}
