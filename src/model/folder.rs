use std::collections::HashSet;
use crate::database::{Connection, Pool};
use futures::prelude::*;
use sqlx::Result;
use tracing::{trace};

#[derive(Debug)]
pub struct Folder {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: Option<String>,
}

impl Folder {
    pub(crate) async fn create(&self, conn: &mut Connection) -> Result<()> {
        sqlx::query!(
            "
            INSERT INTO folders
                (id, drive_id, name, trashed, parent)
            VALUES
                ($1, $2, $3, $4, $5)
            ",
            self.id,
            self.drive_id,
            self.name,
            self.trashed,
            self.parent,
        )
            .execute(conn)
            .await?;

        trace!(id = %self.id, "created folder");
        Ok(())
    }

    pub(crate) async fn get_all(drive_id: &str, conn: &mut Connection) -> Result<Vec<Folder>> {
        let rows = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, drive_id, name, trashed, parent
            FROM folders
            WHERE drive_id = $1
            "#,
            drive_id
        )
            .fetch_all(conn)
            .await?;

        trace!(drive_id = %drive_id, count = %rows.len(), "fetched all folders");
        Ok(rows)
    }

    pub(crate) async fn upsert(&self, conn: &mut Connection) -> Result<()> {
        sqlx::query!(
            "
             UPDATE folders
        SET
            name = $3,
            trashed = $4,
            parent = $5
        WHERE id = $1 AND drive_id = $2
            ",
            self.id,
            self.drive_id,
            self.name,
            self.trashed,
            self.parent,
        )
            .execute(conn)
            .await?;

        trace!(id = %self.id, "upserted folder");
        Ok(())
    }






    pub(crate) async fn delete(id: &str, drive_id: &str, conn: &mut Connection) -> Result<()> {
        sqlx::query!(
            "DELETE FROM folders WHERE id = $1 AND drive_id = $2",
            id,
            drive_id
        )
            .execute(conn)
            .await?;

        trace!(id = %id, "deleted folder");
        Ok(())
    }

    pub(crate) async fn update_name(
        id: &str,
        drive_id: &str,
        name: &str,
        conn: &mut Connection,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE folders SET name = $3 WHERE id = $1 AND drive_id = $2",
            id,
            drive_id,
            name
        )
            .execute(conn)
            .await?;

        trace!(id = %id, "updated folder name to {}", name);
        Ok(())
    }

    pub(crate) async fn get_all_ids(drive_id: &str, conn: &mut Connection) -> Result<HashSet<String>> {
        let rows = sqlx::query!(
            "SELECT id FROM folders WHERE drive_id = $1",
            drive_id
        )
            .fetch_all(conn)
            .await?;

        let ids: HashSet<String> = rows.into_iter().map(|row| row.id).collect();

        trace!(drive_id = %drive_id, count = %ids.len(), "fetched all folder ids");
        Ok(ids)
    }



}

#[derive(Debug)]
pub enum ChangedFolder {
    Created(Folder),
    Deleted(Folder),
}

impl From<ChangedFolder> for Folder {
    fn from(folder: ChangedFolder) -> Self {
        match folder {
            ChangedFolder::Created(folder) => folder,
            ChangedFolder::Deleted(folder) => folder,
        }
    }
}

struct FolderChangelog {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: Option<String>,
    pub deleted: bool,
}

impl From<FolderChangelog> for ChangedFolder {
    fn from(f: FolderChangelog) -> Self {
        let folder = Folder {
            id: f.id,
            drive_id: f.drive_id,
            name: f.name,
            parent: f.parent,
            trashed: f.trashed,
        };

        match f.deleted {
            true => Self::Created(folder),
            false => Self::Deleted(folder),
        }
    }
}

impl ChangedFolder {
    pub(crate) async fn get_all(drive_id: &str, pool: &Pool) -> Result<Vec<Self>> {
        sqlx::query_as!(
            FolderChangelog,
            "SELECT * FROM folder_changelog WHERE drive_id = $1",
            drive_id
        )
            .fetch(pool)
            // Turn the FolderChangelog into a ChangedFolder
            .map_ok(|f| f.into())
            .try_collect()
            .await
    }

    pub(crate) async fn clear(drive_id: &str, pool: &Pool) -> Result<()> {
        sqlx::query!("DELETE FROM folder_changelog WHERE drive_id = $1", drive_id)
            .execute(pool)
            .await?;

        trace!("cleared folder changelog");
        Ok(())
    }
}