use crate::database::{Connection, Pool};
use futures::prelude::*;
use sqlx::{Result, Sqlite, Error};
use tracing::{debug, info, trace, warn};

#[derive(Debug)]
pub struct File {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: String,
    pub md5: String,
    pub size: i64,
}

impl File {
    pub(crate) async fn create(&self, conn: &mut Connection) -> Result<()> {
        info!(id = %self.id, drive_id = %self.drive_id, name = %self.name, "Starting to create file");

        match sqlx::query!(
        "
        INSERT INTO files
            (id, drive_id, name, trashed, parent, md5, size)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7)
        ",
        self.id,
        self.drive_id,
        self.name,
        self.trashed,
        self.parent,
        self.md5,
        self.size
    )
            .execute(conn)
            .await
        {
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
        info!(id = %self.id, drive_id = %self.drive_id, name = %self.name, "upsert to create file");

        // 检查父文件夹是否存在
        if !self.parent.is_empty() {
            let parent_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM folders WHERE id = $1 AND drive_id = $2)")
                .bind(&self.parent)
                .bind(&self.drive_id)
                .fetch_one(&mut *conn)
                .await?;

            if !parent_exists {
                warn!(
                id = %self.id,
                parent_id = %self.parent,
                drive_id = %self.drive_id,
                "Parent folder not found"
            );
                // 根据您的需求，您可能想在这里返回错误
                // return Err(anyhow::anyhow!("Parent folder not found"));
            }
        }

        // 定义文件查询
        let file_query = r#"
    INSERT OR REPLACE INTO files (id, drive_id, name, trashed, parent, md5, size)
    VALUES ($1, $2, $3, $4, $5, $6, $7)
    "#;

        // 执行文件查询
        let result = sqlx::query(file_query)
            .bind(&self.id)
            .bind(&self.drive_id)
            .bind(&self.name)
            .bind(self.trashed)
            .bind(&self.parent)
            .bind(&self.md5)
            .bind(self.size)
            .execute(&mut *conn)
            .await;

        match result {
            Ok(_) => {
                trace!(id = %self.id, "upserted file successfully");
                Ok(())
            }
            Err(e) => {
                warn!(
                id = %self.id,
                error = %e,
                "Failed to upsert file"
            );
                Err(e.into())
            }
        }
    }





    pub(crate) async fn delete(id: &str, drive_id: &str, conn: &mut Connection) -> Result<()> {
        sqlx::query!(
            "DELETE FROM files WHERE id = $1 AND drive_id = $2",
            id,
            drive_id
        )
            .execute(conn)
            .await?;

        trace!(id = %id, "deleted file");
        Ok(())
    }
}

#[derive(Debug)]
pub enum ChangedFile {
    Created(File),
    Deleted(File),
}

impl From<ChangedFile> for File {
    fn from(file: ChangedFile) -> Self {
        match file {
            ChangedFile::Created(file) => file,
            ChangedFile::Deleted(file) => file,
        }
    }
}

struct FileChangelog {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: String,
    pub md5: String,
    pub size: i64,
    pub deleted: bool,
}

impl From<FileChangelog> for ChangedFile {
    fn from(f: FileChangelog) -> Self {
        let file = File {
            id: f.id,
            drive_id: f.drive_id,
            name: f.name,
            parent: f.parent,
            trashed: f.trashed,
            md5: f.md5,
            size: f.size,
        };

        match f.deleted {
            true => Self::Created(file),
            false => Self::Deleted(file),
        }
    }
}

impl ChangedFile {
    pub(crate) async fn get_all(drive_id: &str, pool: &Pool) -> Result<Vec<Self>> {
        sqlx::query_as!(
            FileChangelog,
            "SELECT * FROM file_changelog WHERE drive_id = $1",
            drive_id
        )
            .fetch(pool)
            // Turn the FileChangelog into a ChangedFile
            .map_ok(|f| f.into())
            .try_collect()
            .await
    }

    pub(crate) async fn clear(drive_id: &str, pool: &Pool) -> Result<()> {
        sqlx::query!("DELETE FROM file_changelog WHERE drive_id = $1", drive_id)
            .execute(pool)
            .await?;

        trace!("cleared file changelog");
        Ok(())
    }
}
