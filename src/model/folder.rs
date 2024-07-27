use crate::database::{Connection, Pool};
use futures::prelude::*;
use sqlx::{Result};
use tracing::{info, trace, warn};

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
        info!(id = %self.id, drive_id = %self.drive_id, name = %self.name, "Starting to create folder");

        match sqlx::query!(
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
            .await {
            Ok(_) => {
                trace!(id = %self.id, "Created file successfully");
                Ok(())
            }
            Err(e) => {
                warn!(error = ?e, "Failed to create file");
                Err(e)
            }
        }
    }

    pub(crate) async fn upsert(&self, conn: &mut Connection) -> Result<()> {
        info!(id = %self.id, drive_id = %self.drive_id, name = %self.name, "upsert to create folder");

        // 检查父文件夹是否存在
        if let Some(parent_id) = &self.parent {
            let parent_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1 AND drive_id = $2)")
                .bind(parent_id)
                .bind(&self.drive_id)
                .fetch_one(&mut *conn)
                .await?;

            if !parent_exists {
                warn!(
                id = %self.id,
                parent_id = %parent_id,
                drive_id = %self.drive_id,
                "Parent folder not found"
            );
                // 根据您的需求，您可能想在这里返回错误
                // return Err(anyhow::anyhow!("Parent folder not found"));
            }
        }

        // 如果父文件夹存在或者没有父文件夹，则执行插入或更新操作
        let query = r#"
    INSERT OR REPLACE INTO folders (id, drive_id, name, trashed, parent)
    VALUES ($1, $2, $3, $4, $5)
    "#;

        let result= sqlx::query(query)
            .bind(&self.id)
            .bind(&self.drive_id)
            .bind(&self.name)
            .bind(self.trashed)
            .bind(&self.parent)
            .execute(&mut *conn)
            .await;

        match result {
            Ok(_) => {
                trace!(id = %self.id, "upserted folder successfully");
                Ok(())
            }
            Err(e) => {
                warn!(
                id = %self.id,
                error = %e,
                "Failed to upsert folder"
            );
                Err(e.into())
            }
        }
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
