use crate::fetch::{Change, Item};
use crate::model::{ChangedFile, ChangedFolder, ChangedPath, Drive, File, Folder};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePool, SqlitePoolOptions};
use tracing::trace;

pub(crate) type Connection = SqliteConnection;

pub(crate) type Pool = SqlitePool;

pub async fn establish_connection(database_path: &str) -> sqlx::Result<Pool> {
    let options = SqliteConnectOptions::default()
        .create_if_missing(true)
        .foreign_keys(true)
        .filename(database_path);

    let pool = SqlitePoolOptions::new().connect_with(options).await?;

    sqlx::migrate!().run(&pool).await?;

    Ok(pool)
}

pub async fn clear_changelog(drive_id: &str, pool: &Pool) -> sqlx::Result<()> {
    ChangedFolder::clear(drive_id, pool).await?;
    ChangedFile::clear(drive_id, pool).await?;

    Ok(())
}

async fn delete_file_or_folder(
    id: &str,
    drive_id: &str,
    conn: &mut Connection,
) -> sqlx::Result<()> {
    Folder::delete(id, drive_id, conn).await?;
    File::delete(id, drive_id, conn).await?;

    Ok(())
}

fn item_to_change(drive_id: &str, item: Item) -> Change {
    match item.drive_id() == drive_id {
        true => Change::ItemChanged(item),
        false => {
            trace!("moved to another shared drive, marked as removed");
            Change::ItemRemoved(item.into_id())
        }
    }
}

#[tracing::instrument(level = "debug", skip(changes, pool))]
pub async fn merge_changes<I>(
    drive_id: &str,
    changes: I,
    page_token: &str,
    pool: &Pool,
) -> sqlx::Result<()>
where
    I: IntoIterator<Item = Change>,
{
    let mut tx = pool.begin().await?;

    // First update the page_token
    Drive::update_page_token(drive_id, page_token, &mut tx).await?;

    // If an item changes to another drive_id, consider it removed.
    let changes = changes.into_iter().map(|change| match change {
        Change::ItemChanged(item) => item_to_change(drive_id, item),
        _ => change,
    });

    for change in changes {
        match change {
            Change::DriveChanged(drive) => {
                Folder::update_name(drive_id, drive_id, &drive.name, &mut tx).await?
            }
            Change::ItemChanged(item) => match item {
                Item::File(file) => file.upsert(&mut tx).await?,
                Item::Folder(folder) => folder.upsert(&mut tx).await?,
            },
            Change::ItemRemoved(id) => delete_file_or_folder(&id, drive_id, &mut tx).await?,
            Change::DriveRemoved(_) => (),
        }
    }

    tx.commit().await
}

#[tracing::instrument(level = "debug", skip(name, items, pool))]
pub async fn add_drive<I>(
    drive_id: &str,
    name: &str,
    page_token: &str,
    items: I,
    pool: &Pool,
) -> sqlx::Result<()>
where
    I: IntoIterator<Item = Item>,
{
    let mut tx = pool.begin().await?;

    Drive::create(drive_id, page_token, &mut tx).await?;

    let drive_folder = Folder {
        id: drive_id.to_owned(),
        drive_id: drive_id.to_owned(),
        name: name.to_owned(),
        parent: None,
        trashed: false,
    };

    drive_folder.create(&mut tx).await?;

    for item in items {
        match item {
            Item::File(file) => file.create(&mut tx).await?,
            Item::Folder(folder) => folder.create(&mut tx).await?,
        }
    }

    // Explicitly commit (otherwise this would rollback on drop)
    tx.commit().await
}

pub async fn get_drive(drive_id: &str, pool: &Pool) -> sqlx::Result<Option<Drive>> {
    Drive::get_by_id(drive_id, pool).await
}

pub async fn get_changed_files(drive_id: &str, pool: &Pool) -> sqlx::Result<Vec<ChangedFile>> {
    ChangedFile::get_all(drive_id, pool).await
}

pub async fn get_changed_folders(drive_id: &str, pool: &Pool) -> sqlx::Result<Vec<ChangedFolder>> {
    ChangedFolder::get_all(drive_id, pool).await
}

pub async fn get_changed_paths(drive_id: &str, pool: &Pool) -> sqlx::Result<Vec<ChangedPath>> {
    ChangedPath::get_all(drive_id, pool).await
}
