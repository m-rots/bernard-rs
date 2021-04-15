use crate::schema::*;
use diesel::{sqlite::Sqlite, Queryable};

#[derive(Debug, Insertable, AsChangeset, Queryable)]
#[table_name = "files"]
pub struct File {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: String,
    pub md5: String,
    pub size: i64,
}

#[derive(Insertable)]
#[table_name = "files"]
pub struct NewFile<'a> {
    pub id: &'a str,
    pub drive_id: &'a str,
    pub name: &'a str,
    pub trashed: bool,
    pub parent: Option<&'a str>,
    pub md5: &'a str,
    pub size: i64,
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

impl Queryable<file_changelog::SqlType, Sqlite> for ChangedFile {
    type Row = (String, String, bool, String, bool, String, String, i64);

    fn build(
        (id, drive_id, deleted, name, trashed, parent, md5, size): Self::Row,
    ) -> diesel::deserialize::Result<Self> {
        let file = File {
            id,
            drive_id,
            name,
            trashed,
            parent,
            md5,
            size,
        };

        match deleted {
            true => Ok(Self::Deleted(file)),
            false => Ok(Self::Created(file)),
        }
    }
}
