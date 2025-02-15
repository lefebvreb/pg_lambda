use pg_lambda::schema::{Schema, Table};

#[derive(Table)]
#[table(check = "age >= 18")]
pub struct User {
    #[table(primary_key)]
    pub id: i32,
    #[table(unique)]
    pub email: String,
    pub name: String,
    pub age: i32,
}

#[derive(Table)]
#[table(schema = "public")]
pub struct Team {
    #[table(primary_key)]
    pub id: i32,
}

#[derive(Table)]
#[table(primary_key = "(team_id, user_id)")]
pub struct TeamUser {
    #[table(foreign_key = "public.Team (id)")]
    pub team_id: i32,
    #[table(foreign_key = "public.Team (id)")]
    pub user_id: i32,
}

#[derive(Table)]
#[table(foreign_key = "(team_id, user_id) TeamUser (team_id, user_id) cascade)")]
pub struct Permission {
    pub team_id: i32,
    pub user_id: i32,
}

#[test]
fn schema() {
    let s = serde_json::to_string_pretty(Schema::get()).unwrap();
    std::fs::write(".vscode/schema.json", s).unwrap();
}
