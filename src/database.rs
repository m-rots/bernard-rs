use crate::fetch::{Change, Item};
use crate::model::{ChangedFile, ChangedFolder, ChangedPath, Drive, File, Folder};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePool, SqlitePoolOptions};
use tracing::{trace, warn};

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

    // Update the page_token
    Drive::update_page_token(drive_id, page_token, &mut tx).await?;

    // Process changes
    for change in changes.into_iter() {
        let change = match change {
            Change::ItemChanged(item) => item_to_change(drive_id, item),
            other => other, // 使用 catch-all 模式替代 '*'
        };

        match change {
            Change::DriveChanged(drive) => {
                if let Err(e) = Folder::update_name(drive_id, drive_id, &drive.name, &mut tx).await {
                    warn!(error = %e, "Failed to update drive name");
                    // 继续处理其他更改
                }
            }
            Change::ItemChanged(Item::Folder(folder)) => {
                if let Err(e) = folder.upsert(&mut tx).await {
                    warn!(id = %folder.id, error = %e, "Failed to upsert folder");
                    // 继续处理其他更改
                }
            }
            Change::ItemChanged(Item::File(file)) => {
                if let Err(e) = file.upsert(&mut tx).await {
                    warn!(id = %file.id, error = %e, "Failed to upsert file");
                    // 继续处理其他更改
                }
            }
            Change::ItemRemoved(id) => {
                if let Err(e) = delete_file_or_folder(&id, drive_id, &mut tx).await {
                    warn!(id = %id, error = %e, "Failed to delete file or folder");
                    // 继续处理其他更改
                }
            }
            Change::DriveRemoved(_) => (),
        }
    }

    // 尝试提交事务
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

    // Collect items into a Vec
    let items: Vec<Item> = items.into_iter().collect();

    // Create a HashSet to store existing folder IDs
    let mut existing_folders = std::collections::HashSet::new();
    existing_folders.insert(drive_id.to_owned());

    // First pass: Process all folders
    for item in &items {
        if let Item::Folder(folder) = item {
            // Check if parent folder exists before creating
            if folder.parent.as_ref().map_or(true, |parent| existing_folders.contains(parent)) {
                folder.create(&mut tx).await?;
                existing_folders.insert(folder.id.clone());
            } else {
                // Log a warning or handle the case where parent folder doesn't exist
                tracing::warn!("Parent folder {:?} doesn't exist for folder {}", folder.parent, folder.id);
            }
        }
    }

    // Second pass: Process all files
    for item in &items {
        if let Item::File(file) = item {
            // Check if parent folder exists before creating the file
            if existing_folders.contains(&file.parent) {
                file.create(&mut tx).await?;
            } else {
                // Log a warning or handle the case where parent folder doesn't exist
                tracing::warn!("Parent folder {} doesn't exist for file {}", file.parent, file.id);
            }
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
