use crate::schema::*;
use diesel::{query_dsl::methods::FilterDsl, sqlite::Sqlite, ExpressionMethods, Queryable};

#[derive(Debug, Insertable, AsChangeset, Queryable)]
#[table_name = "folders"]
pub struct Folder {
    pub id: String,
    pub drive_id: String,
    pub name: String,
    pub trashed: bool,
    pub parent: Option<String>,
}

#[derive(Insertable)]
#[table_name = "folders"]
pub struct NewFolder<'a> {
    pub id: &'a str,
    pub drive_id: &'a str,
    pub name: &'a str,
    pub trashed: bool,
    pub parent: Option<&'a str>,
}

#[derive(Debug)]
pub enum ChangedFolder {
    Created(Folder),
    Deleted(Folder),
}

type WithDrive<'a> = diesel::dsl::Eq<folder_changelog::drive_id, &'a str>;
type ByDrive<'a> = diesel::dsl::Filter<folder_changelog::table, WithDrive<'a>>;

impl ChangedFolder {
    pub(crate) fn with_drive(drive_id: &str) -> WithDrive<'_> {
        folder_changelog::drive_id.eq(drive_id)
    }

    pub(crate) fn by_drive(drive_id: &str) -> ByDrive<'_> {
        folder_changelog::table.filter(Self::with_drive(drive_id))
    }
}

impl From<ChangedFolder> for Folder {
    fn from(folder: ChangedFolder) -> Self {
        match folder {
            ChangedFolder::Created(folder) => folder,
            ChangedFolder::Deleted(folder) => folder,
        }
    }
}

impl Queryable<folder_changelog::SqlType, Sqlite> for ChangedFolder {
    type Row = (String, String, bool, String, bool, Option<String>);

    fn build(
        (id, drive_id, deleted, name, trashed, parent): Self::Row,
    ) -> diesel::deserialize::Result<Self> {
        let folder = Folder {
            id,
            drive_id,
            name,
            trashed,
            parent,
        };

        match deleted {
            true => Ok(Self::Deleted(folder)),
            false => Ok(Self::Created(folder)),
        }
    }
}
