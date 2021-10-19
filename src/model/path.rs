use std::path::PathBuf;

use crate::database::Pool;
use futures::prelude::*;

#[derive(Debug)]
pub enum Path {
    File(InnerPath),
    Folder(InnerPath),
}

impl Path {
    pub fn trashed(&self) -> bool {
        match self {
            Self::File(inner) => inner.trashed,
            Self::Folder(inner) => inner.trashed,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct InnerPath {
    pub id: String,
    pub drive_id: String,
    pub path: PathBuf,
    pub trashed: bool,
}

#[derive(Debug)]
pub enum ChangedPath {
    Created(Path),
    Deleted(Path),
}

impl From<ChangedPath> for Path {
    fn from(path: ChangedPath) -> Self {
        match path {
            ChangedPath::Created(path) => path,
            ChangedPath::Deleted(path) => path,
        }
    }
}

impl From<ChangedPath> for InnerPath {
    fn from(path: ChangedPath) -> Self {
        match path {
            ChangedPath::Created(path) => path.into(),
            ChangedPath::Deleted(path) => path.into(),
        }
    }
}

impl From<Path> for InnerPath {
    fn from(path: Path) -> Self {
        match path {
            Path::File(inner) => inner,
            Path::Folder(inner) => inner,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PathChangelog {
    pub id: String,
    pub drive_id: String,
    pub path: String,
    pub folder: bool,
    pub deleted: bool,
    pub trashed: bool,
}

impl From<PathChangelog> for Path {
    fn from(p: PathChangelog) -> Self {
        let inner_path = InnerPath {
            id: p.id,
            drive_id: p.drive_id,
            path: p.path.into(),
            trashed: p.trashed,
        };

        match p.folder {
            true => Path::Folder(inner_path),
            false => Path::File(inner_path),
        }
    }
}

impl From<PathChangelog> for ChangedPath {
    fn from(path: PathChangelog) -> Self {
        match path.deleted {
            true => Self::Deleted(path.into()),
            false => Self::Created(path.into()),
        }
    }
}

impl ChangedPath {
    pub(crate) async fn get_all(drive_id: &str, pool: &Pool) -> sqlx::Result<Vec<Self>> {
        // TODO: SQLx appears to have a bug with Recursive CTEs (even if it's just a view).
        // Therefore this query is not checked.
        // Maybe open an issue or investigate what goes wrong?
        sqlx::query_as::<_, PathChangelog>("SELECT * FROM path_changelog WHERE drive_id = $1")
            .bind(drive_id)
            .fetch(pool)
            // Turn the PathChangelog into a ChangedPath
            .map_ok(|f| f.into())
            .try_collect()
            .await
    }
}
