use anyhow::Result;
use bernard::{Account, Bernard, SyncKind};
use clap::Clap;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

/// A basic example showing you how to synchronise one Shared Drive at a time.
#[derive(Clap)]
#[clap(name = "basic")]
struct Opt {
    #[clap(short, long, value_name = "PATH", default_value = "account.json")]
    account: String,

    /// Database file path
    #[clap(
        long = "database",
        alias = "db",
        value_name = "PATH",
        default_value = "example-basic.db"
    )]
    database_path: String,

    /// Shared Drive ID to synchronise
    #[clap(short, long = "drive", value_name = "ID")]
    drive_id: String,

    /// Route all HTTP traffic through a proxy
    #[clap(short, long, value_name = "URL")]
    proxy: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse the CLI arguments.
    let opt = Opt::parse();

    // Set up the trace logging.
    let filter = EnvFilter::from_default_env().add_directive("bernard=info".parse()?);

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .pretty()
        .init();

    // Read the Service Account JSON Key file.
    let account = Account::from_file(&opt.account)?;

    // Begin building Bernard.
    let mut bernard = Bernard::builder(&opt.database_path, account);

    // Set the proxy if one was provided.
    if let Some(url) = opt.proxy {
        bernard = bernard.proxy(&url);
    }

    // Build complete!
    let bernard = bernard.build().await.unwrap();

    // Sync the provided Shared Drive
    match bernard.sync_drive(&opt.drive_id).await? {
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
