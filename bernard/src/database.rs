use crate::{fetch::Item, schema};
use crate::{
    fetch::{Change, PartialDrive},
    model::{ChangedFile, ChangedFolder, ChangedPath, Drive, File, Folder, NewDrive, NewFolder},
};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind;
use diesel::result::Error as DieselError;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use snafu::{ResultExt, Snafu};
use std::time::Instant;
use tracing::{debug, trace};

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
        source: DieselError,
    },
    #[snafu(display("Could not upsert Folder `{}` in Drive `{}`", id, drive_id))]
    FolderUpsert {
        id: String,
        drive_id: String,
        source: DieselError,
    },
    #[snafu(display("Unknown"))]
    Unknown { source: DieselError },
    #[snafu(display("Failed to enforce database integrity"))]
    DataIntegrity { source: DieselError },
}

// TODO: Better match error kinds?
// or just expose more errors
impl From<DieselError> for Error {
    fn from(source: DieselError) -> Self {
        match source {
            DieselError::DatabaseError(kind, _) => match kind {
                DatabaseErrorKind::ForeignKeyViolation => Self::DataIntegrity { source },
                _ => Self::Unknown { source },
            },
            _ => Self::Unknown { source },
        }
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

    trace!("cleared folder changelog");
    Ok(())
}

fn clear_file_changelog(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    use schema::file_changelog;

    diesel::delete(file_changelog::table)
        .filter(file_changelog::drive_id.eq(drive_id))
        .execute(conn)?;

    trace!("cleared file changelog");
    Ok(())
}

pub fn clear_changelog(conn: &SqliteConnection, drive_id: &str) -> Result<()> {
    clear_file_changelog(conn, drive_id)?;
    clear_folder_changelog(conn, drive_id)?;

    debug!("cleared changelog");
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

#[tracing::instrument(skip(conn, folder), fields(id = ?folder.id, parent = ?folder.parent))]
fn upsert_folder(conn: &SqliteConnection, folder: Folder) -> Result<()> {
    use schema::folders;

    let Folder { id, drive_id, .. } = &folder;

    diesel::insert_into(folders::table)
        .values(&folder)
        .on_conflict((folders::id, folders::drive_id))
        .do_update()
        .set(&folder)
        .execute(conn)
        .context(FolderUpsert { id, drive_id })?;

    trace!("upserted folder");
    Ok(())
}

#[tracing::instrument(skip(conn, file), fields(id = ?file.id, parent = ?file.parent))]
fn upsert_file(conn: &SqliteConnection, file: File) -> Result<()> {
    use schema::files;

    let File { id, drive_id, .. } = &file;

    diesel::insert_into(files::table)
        .values(&file)
        .on_conflict((files::id, files::drive_id))
        .do_update()
        .set(&file)
        .execute(conn)
        .context(FileUpsert { id, drive_id })?;

    trace!("upserted file");
    Ok(())
}

#[tracing::instrument(skip(conn, drive), fields(id = ?drive.id, name = ?drive.name))]
fn update_drive_name(conn: &SqliteConnection, drive: PartialDrive) -> Result<()> {
    use schema::folders;

    diesel::update(folders::table)
        .filter(folders::id.eq(&drive.id))
        .set(folders::name.eq(drive.name))
        .execute(conn)?;

    trace!("updated drive name");
    Ok(())
}

#[tracing::instrument(skip(conn, drive_id))]
fn delete_file_or_folder(conn: &SqliteConnection, id: &str, drive_id: &str) -> Result<()> {
    use schema::{files, folders};

    diesel::delete(folders::table)
        .filter(folders::id.eq(&id).and(folders::drive_id.eq(drive_id)))
        .execute(conn)?;

    diesel::delete(files::table)
        .filter(files::id.eq(&id).and(files::drive_id.eq(drive_id)))
        .execute(conn)?;

    trace!("deleted file/folder");
    Ok(())
}

#[tracing::instrument(skip(drive_id, item), fields(id = ?item.id(), drive_id = ?item.drive_id()))]
fn item_to_change(drive_id: &str, item: Item) -> Change {
    match item.drive_id() == drive_id {
        true => Change::ItemChanged(item),
        false => {
            trace!("moved to another shared drive, marked as removed");
            Change::ItemRemoved(item.into_id())
        }
    }
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
    let start = Instant::now();

    conn.transaction::<_, Error, _>(|| {
        // First update the page_token
        update_page_token(conn, drive_id, page_token)?;

        // If an item changes to another drive_id, consider it removed.
        let changes = changes.into_iter().map(|change| match change {
            Change::ItemChanged(item) => item_to_change(drive_id, item),
            _ => change,
        });

        for change in changes {
            match change {
                Change::DriveChanged(drive) => update_drive_name(conn, drive)?,
                Change::ItemChanged(item) => match item {
                    Item::File(file) => upsert_file(conn, file)?,
                    Item::Folder(folder) => upsert_folder(conn, folder)?,
                },
                Change::ItemRemoved(id) => delete_file_or_folder(conn, &id, drive_id)?,
                Change::DriveRemoved(_) => (),
            }
        }

        trace!("end of transaction");
        Ok(())
    })?;

    debug!(duration = ?start.elapsed(), "changes merged");
    Ok(())
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

pub fn get_changed_folders_paths(
    conn: &SqliteConnection,
    drive_id: &str,
) -> Result<Vec<(ChangedFolder, ChangedPath)>> {
    use schema::{folder_changelog as folders, path_changelog as paths};

    let start = Instant::now();

    let changed_folders = folders::table
        .filter(folders::drive_id.eq(drive_id))
        .inner_join(
            paths::table.on(paths::id.eq(folders::id).and(
                paths::drive_id
                    .eq(folders::drive_id)
                    .and(paths::deleted)
                    .eq(folders::deleted),
            )),
        )
        .load(conn)?;

    debug!(elapsed = ?start.elapsed(), "retrieved changed folders with paths");

    Ok(changed_folders)
}

pub fn get_changed_files_paths(
    conn: &SqliteConnection,
    drive_id: &str,
) -> Result<Vec<(ChangedFile, ChangedPath)>> {
    use schema::{file_changelog as files, path_changelog as paths};

    let start = Instant::now();

    let changed_files = files::table
        .filter(files::drive_id.eq(drive_id))
        .inner_join(
            paths::table.on(paths::id.eq(files::id).and(
                paths::drive_id
                    .eq(files::drive_id)
                    .and(paths::deleted)
                    .eq(files::deleted),
            )),
        )
        .load(conn)?;

    debug!(elapsed = ?start.elapsed(), "retrieved changed files with paths");

    Ok(changed_files)
}
