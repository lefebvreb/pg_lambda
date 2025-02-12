use pg_lambda::pg_lambda;

pg_lambda! {
    pub fn double(x: Integer) -> Integer {
        RETURN QUERY SELECT "x" * 2;
    }
}

#[test]
fn lambda() {}
