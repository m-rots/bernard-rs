use bernard::{Account, Bernard, SyncKind};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the Service Account JSON Key from a file.
    let account = Account::from_file("account.json")?;

    // Build Bernard.
    let bernard = Bernard::builder("bernard.db", account).build().await?;

    // Sync the provided Shared Drive.
    // Replace the drive_id with a Shared Drive ID your service account has access to.
    match bernard.sync_drive("0A1xxxxxxxxxUk9PVA").await? {
        // Do not do anything on a full-sync.
        SyncKind::Full => (),

        // Print the changes this partial sync fetched.
        SyncKind::Partial(changes) => {
            let paths = changes.paths().await?;
            println!("changed paths: {:#?}", paths);
        }
    }

    // Close Bernard's internal connection pool.
    // This is required to clean up the .wal and .shm files on shutdown.
    bernard.close().await;

    Ok(())
}
