use crate::schema::*;

#[derive(Debug, Insertable, Queryable)]
#[table_name = "drives"]
pub struct Drive {
    pub id: String,
    pub page_token: String,
}

#[derive(Insertable)]
#[table_name = "drives"]
pub struct NewDrive<'a> {
    pub id: &'a str,
    pub page_token: &'a str,
}
