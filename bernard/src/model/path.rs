use crate::schema::path_changelog;
use diesel::{sqlite::Sqlite, Queryable};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Path {
    pub id: String,
    pub drive_id: String,
    pub trashed: bool,
    pub path: PathBuf,
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

impl Queryable<path_changelog::SqlType, Sqlite> for ChangedPath {
    // id, drive_id, deleted, path
    type Row = (String, String, bool, bool, String);

    fn build(
        (id, drive_id, deleted, trashed, path): Self::Row,
    ) -> diesel::deserialize::Result<Self> {
        let path = PathBuf::from(path);
        let path = Path {
            id,
            drive_id,
            trashed,
            path,
        };

        match deleted {
            true => Ok(Self::Deleted(path)),
            false => Ok(Self::Created(path)),
        }
    }
}
