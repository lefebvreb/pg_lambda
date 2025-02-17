use pg_lambda::lambda::pg_lambda;
use pg_lambda::types::{Integer, Text};

pg_lambda! {
    pub fn function1(x: Integer) -> Integer r#"
        RETURN QUERY SELECT "x" * 2;
    "#

    pub fn procedure1() -> Text r#""#

    pub fn procedure2() r#""#
}

#[test]
fn lambda() {}
