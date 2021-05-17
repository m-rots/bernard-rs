use crate::database::{Connection, Pool};
use futures::prelude::*;
use sqlx::Result;
use tracing::trace;

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
        sqlx::query!(
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
        .await?;

        trace!(id = %self.id, "created file");
        Ok(())
    }

    pub(crate) async fn upsert(&self, conn: &mut Connection) -> Result<()> {
        sqlx::query!(
            "
            INSERT INTO files
                (id, drive_id, name, trashed, parent, md5, size)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id, drive_id) DO UPDATE SET
                name = EXCLUDED.name,
                trashed = EXCLUDED.trashed,
                parent = EXCLUDED.parent,
                md5 = EXCLUDED.md5,
                size = EXCLUDED.size
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
        .await?;

        trace!(id = %self.id, "upserted file");
        Ok(())
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
