use pg_lambda::schema::Table;
use pg_lambda::types::{Integer, Text};

#[derive(Table)]
#[table(check = "age >= 18")]
pub struct User {
    #[table(primary_key)]
    pub id: Integer,
    #[table(unique)]
    pub email: Text,
    pub name: Text,
    pub age: Integer,
}

#[derive(Table)]
#[table(schema = "public")]
pub struct Team {
    #[table(primary_key)]
    pub id: Integer,
}

#[derive(Table)]
#[table(primary_key = "(team_id, user_id)")]
pub struct TeamUser {
    #[table(foreign_key = "public.Team (id)")]
    pub team_id: Integer,
    #[table(foreign_key = "public.Team (id)")]
    pub user_id: Integer,
}

#[derive(Table)]
#[table(foreign_key = "(team_id, user_id) TeamUser (team_id, user_id) cascade)")]
pub struct Permission {
    pub team_id: Integer,
    pub user_id: Integer,
}

#[test]
fn schema() {}
