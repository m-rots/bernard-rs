use crate::{
    fetch::Change,
    model::{ChangedFile, ChangedFolder, ChangedPath, Drive, NewDrive, NewFolder},
};
use crate::{fetch::Item, schema};
use diesel::prelude::*;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use snafu::{ResultExt, Snafu};

pub use diesel::sqlite::SqliteConnection;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not connect to `{}`", database_path))]
    ConnectionError {
        database_path: String,
        source: diesel::result::ConnectionError,
    },
    #[snafu(display("Could not migrate the database"))]
    MigrationError {
        // Diesel's migration error is really ugly >:(
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("Could not upsert File `{}` in Drive `{}`", id, drive_id))]
    FileUpsert {
        id: String,
        drive_id: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Could not upsert Folder `{}` in Drive `{}`", id, drive_id))]
    FolderUpsert {
        id: String,
        drive_id: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Unknown"))]
    Unknown { source: diesel::result::Error },
}

// TODO: Better match error kinds?
// or just expose more errors
impl From<diesel::result::Error> for Error {
    fn from(e: diesel::result::Error) -> Self {
        Self::Unknown { source: e }
    }
}

type Result<T> = std::result::Result<T, Error>;

pub fn establish_connection(database_path: &str) -> Result<SqliteConnection> {
    let conn =
        SqliteConnection::establish(&database_path).context(ConnectionError { database_path })?;

    // Must manually enable foreign key constraints for every connection.
    conn.execute("PRAGMA foreign_keys = ON")?;

    Ok(conn)
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn run_migration(conn: &SqliteConnection) -> Result<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .context(MigrationError)?;

    Ok(())
}

fn clear_folders(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    use schema::folders;

    diesel::delete(folders::table)
        .filter(folders::drive_id.eq(drive_id))
        .execute(conn)?;

    Ok(())
}

fn clear_files(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    use schema::files;

    diesel::delete(files::table)
        .filter(files::drive_id.eq(drive_id))
        .execute(conn)?;

    Ok(())
}

fn clear_folder_changelog(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    use schema::folder_changelog;

    diesel::delete(folder_changelog::table)
        .filter(folder_changelog::drive_id.eq(drive_id))
        .execute(conn)?;

    Ok(())
}

fn clear_file_changelog(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    use schema::file_changelog;

    diesel::delete(file_changelog::table)
        .filter(file_changelog::drive_id.eq(drive_id))
        .execute(conn)?;

    Ok(())
}

pub fn clear_changelog(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    clear_file_changelog(conn, drive_id)?;
    clear_folder_changelog(conn, drive_id)?;

    Ok(())
}

fn clear_content(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    clear_files(conn, drive_id)?;
    clear_folders(conn, drive_id)?;

    Ok(())
}

fn delete_drive(conn: &SqliteConnection, id: &str) -> Result<()> {
    use schema::drives::dsl;

    diesel::delete(dsl::drives)
        .filter(dsl::id.eq(id))
        .execute(conn)?;

    Ok(())
}

pub fn remove_drive(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    conn.transaction::<_, Error, _>(|| {
        clear_changelog(conn, drive_id)?;
        clear_content(conn, drive_id)?;
        clear_changelog(conn, drive_id)?;
        delete_drive(conn, drive_id)?;

        Ok(())
    })?;

    Ok(())
}

pub fn add_content<I>(conn: &SqliteConnection, items: I) -> Result<()>
where
    I: IntoIterator<Item = Item>,
{
    conn.transaction::<_, Error, _>(|| {
        for item in items {
            match item {
                Item::File(file) => {
                    diesel::insert_into(schema::files::table)
                        .values(file)
                        .execute(conn)?;
                }
                Item::Folder(folder) => {
                    diesel::insert_into(schema::folders::table)
                        .values(folder)
                        .execute(conn)?;
                }
            }
        }

        Ok(())
    })?;

    Ok(())
}

fn update_page_token(conn: &SqliteConnection, drive_id: &str, page_token: &str) -> Result<()> {
    use schema::drives;

    diesel::update(drives::table)
        .filter(drives::id.eq(drive_id))
        .set(drives::page_token.eq(page_token))
        .execute(conn)?;

    Ok(())
}

pub fn merge_changes<I>(
    conn: &SqliteConnection,
    drive_id: &str,
    changes: I,
    page_token: &str,
) -> Result<()>
where
    I: IntoIterator<Item = Change>,
{
    conn.transaction::<_, Error, _>(|| {
        // First update the page_token
        update_page_token(conn, drive_id, page_token)?;

        // If an item changes to another drive_id, consider it removed.
        let changes = changes.into_iter().map(|change| match change {
            Change::ItemChanged(item) => match item.drive_id() == drive_id {
                true => Change::ItemChanged(item),
                false => Change::ItemRemoved(item.into_id()),
            },
            _ => change,
        });

        for change in changes {
            match change {
                Change::DriveChanged(drive) => {
                    use schema::folders;

                    diesel::update(folders::table)
                        .filter(folders::id.eq(&drive.id))
                        .set(folders::name.eq(drive.name))
                        .execute(conn)?;
                }
                Change::ItemChanged(item) => match item {
                    Item::File(file) => {
                        use schema::files;

                        diesel::insert_into(files::table)
                            .values(&file)
                            .on_conflict((files::id, files::drive_id))
                            .do_update()
                            .set(&file)
                            .execute(conn)
                            .context(FileUpsert {
                                id: &file.id,
                                drive_id,
                            })?;
                    }
                    Item::Folder(folder) => {
                        use schema::folders;

                        diesel::insert_into(folders::table)
                            .values(&folder)
                            .on_conflict((folders::id, folders::drive_id))
                            .do_update()
                            .set(&folder)
                            .execute(conn)
                            .context(FolderUpsert {
                                id: &folder.id,
                                drive_id,
                            })?;
                    }
                },
                Change::ItemRemoved(id) => {
                    use schema::{files, folders};

                    diesel::delete(folders::table)
                        .filter(folders::id.eq(&id).and(folders::drive_id.eq(drive_id)))
                        .execute(conn)?;

                    diesel::delete(files::table)
                        .filter(files::id.eq(&id).and(files::drive_id.eq(drive_id)))
                        .execute(conn)?;
                }
                Change::DriveRemoved(_) => (),
            }
        }

        Ok(())
    })
}

pub fn add_drive(conn: &SqliteConnection, id: &str, name: &str, page_token: &str) -> Result<()> {
    conn.transaction::<_, Error, _>(|| {
        diesel::insert_into(schema::drives::table)
            .values(NewDrive { id, page_token })
            .execute(conn)?;

        diesel::insert_into(schema::folders::table)
            .values(NewFolder {
                drive_id: id,
                id,
                name,
                parent: None,
                trashed: false,
            })
            .execute(conn)?;

        Ok(())
    })
}

pub fn get_drive(conn: &SqliteConnection, drive_id: &str) -> Result<Option<Drive>> {
    use schema::drives::dsl::*;

    let drive = drives.find(drive_id).first(conn).optional()?;

    Ok(drive)
}

pub fn get_changed_folders(conn: &SqliteConnection, drive_id: &str) -> Result<Vec<ChangedFolder>> {
    use schema::folder_changelog;

    let changed_folders = folder_changelog::table
        .filter(folder_changelog::drive_id.eq(drive_id))
        .load(conn)?;

    Ok(changed_folders)
}

pub fn get_changed_files(conn: &SqliteConnection, drive_id: &str) -> Result<Vec<ChangedFile>> {
    use schema::file_changelog;

    let changed_files = file_changelog::table
        .filter(file_changelog::drive_id.eq(drive_id))
        .load(conn)?;

    Ok(changed_files)
}

pub fn get_changed_paths(conn: &SqliteConnection, drive_id: &str) -> Result<Vec<ChangedPath>> {
    use schema::path_changelog;

    let changed_paths = path_changelog::table
        .filter(path_changelog::drive_id.eq(drive_id))
        .load(conn)?;

    Ok(changed_paths)
}
