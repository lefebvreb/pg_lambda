use pg_lambda::lambda::pg_lambda;
use pg_lambda::types::Integer;

pg_lambda! {
    pub fn double(x: Integer) -> Integer r#"
        RETURN QUERY SELECT "x" * 2;
    "#

    // pub fn procedure1() r#"

    // "#

    // pub fn procedure2() r#""#
}

// Recursive expansion of pg_lambda! macro
// ========================================

#[test]
fn lambda() {
    for def in inventory::iter::<pg_lambda::__proc_macro_util::PgLambdaDef> {
        println!("{}", def.create_statement)
    }
}
