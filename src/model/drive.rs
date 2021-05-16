use crate::database::{Connection, Pool};

#[derive(Debug)]
pub struct Drive {
    pub id: String,
    pub page_token: String,
}

impl Drive {
    pub(crate) async fn create(
        id: &str,
        page_token: &str,
        conn: &mut Connection,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "INSERT INTO drives (id, page_token) VALUES ($1, $2)",
            id,
            page_token
        )
        .execute(conn)
        .await?;

        Ok(())
    }

    pub(crate) async fn get_by_id(id: &str, pool: &Pool) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, "SELECT * FROM drives WHERE id = $1", id)
            .fetch_optional(pool)
            .await
    }

    pub(crate) async fn update_page_token(
        id: &str,
        page_token: &str,
        conn: &mut Connection,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE drives SET page_token = $2 WHERE id = $1",
            id,
            page_token,
        )
        .execute(conn)
        .await?;

        Ok(())
    }
}
