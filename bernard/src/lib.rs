#[macro_use]
extern crate diesel;

use auth::Account;
use database::SqliteConnection;
use fetch::{FetchBuilder, Fetcher};
use model::Drive;
use reqwest::IntoUrl;
use snafu::Snafu;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::block_in_place;
use tracing::debug;

// TODO: Make auth its own crate + errors
pub mod auth;
mod database;
mod fetch;
mod model;
mod schema;

pub use model::{ChangedFile, ChangedFolder, ChangedPath, File, Folder, Path};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("Database"))]
    Database { source: database::Error },
    #[snafu(display("Network"))]
    Network { source: fetch::Error },
    #[snafu(display("Received a partial change list from Google"))]
    PartialChangeList { source: database::Error },
}

impl From<database::Error> for Error {
    fn from(error: database::Error) -> Self {
        match error {
            database::Error::DataIntegrityError { .. } => Self::PartialChangeList { source: error },
            _ => Self::Database { source: error },
        }
    }
}

impl From<fetch::Error> for Error {
    fn from(source: fetch::Error) -> Self {
        Self::Network { source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Bernard {
    conn: SqliteConnection,
    fetch: Arc<Fetcher>,
}

impl Bernard {
    pub fn builder<S: Into<String>>(database_path: S, account: Account) -> BernardBuilder {
        BernardBuilder::new(database_path, account)
    }

    async fn fill_drive(&self, drive_id: &str) -> Result<()> {
        let items = self.fetch.clone().all_files(drive_id).await?;
        block_in_place(|| database::add_content(&self.conn, items))?;

        Ok(())
    }

    async fn initialise_drive(&self, drive_id: &str) -> Result<()> {
        let page_token = self.fetch.clone().start_page_token(drive_id).await?;
        let name = self.fetch.clone().drive_name(drive_id).await?;

        block_in_place(|| database::add_drive(&self.conn, drive_id, &name, &page_token))?;

        Ok(())
    }

    async fn add_drive(&self, drive_id: &str) -> Result<()> {
        self.initialise_drive(drive_id).await?;
        self.fill_drive(drive_id).await?;
        block_in_place(|| database::clear_changelog(&self.conn, drive_id))?;

        Ok(())
    }

    /// Async wrapper of [clear_changelog](database::clear_changelog).
    pub async fn clear_changelog(&self, drive_id: &str) -> Result<()> {
        block_in_place(|| database::clear_changelog(&self.conn, &drive_id).map_err(|e| e.into()))
    }

    /// Async wrapper of [get_drive](database::get_drive).
    async fn get_drive(&self, drive_id: &str) -> Result<Option<Drive>> {
        block_in_place(|| database::get_drive(&self.conn, drive_id).map_err(|e| e.into()))
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_drive(&self, drive_id: &str) -> Result<()> {
        // Always clear changelog for consistent database state when sync_drive is called.
        self.clear_changelog(drive_id).await?;
        let drive = self.get_drive(drive_id).await?;

        match drive {
            None => {
                debug!("starting full synchronisation");
                self.add_drive(&drive_id).await?;
            }
            Some(drive) => {
                debug!("starting partial synchronisation");

                let (changes, page_token) = self
                    .fetch
                    .clone()
                    .changes(&drive_id, &drive.page_token)
                    .await?;

                // Do not perform database operation if no changes are available.
                if page_token == drive.page_token {
                    debug!(page_token = %page_token, "page token has not changed");
                    return Ok(());
                }

                block_in_place(|| {
                    database::merge_changes(&self.conn, &drive_id, changes, &page_token)
                })?;
            }
        };

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn remove_drive(&self, drive_id: &str) -> Result<()> {
        database::remove_drive(&self.conn, drive_id)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn get_changed_folders(&self, drive_id: &str) -> Result<Vec<ChangedFolder>> {
        let changed_folders = database::get_changed_folders(&self.conn, drive_id)?;
        Ok(changed_folders)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_changed_files(&self, drive_id: &str) -> Result<Vec<ChangedFile>> {
        let changed_files = database::get_changed_files(&self.conn, drive_id)?;
        Ok(changed_files)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_changed_paths(&self, drive_id: &str) -> Result<Vec<ChangedPath>> {
        let changed_paths = database::get_changed_paths(&self.conn, drive_id)?;
        Ok(changed_paths)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_changed_folders_paths(
        &self,
        drive_id: &str,
    ) -> Result<impl Iterator<Item = (ChangedFolder, PathBuf)>> {
        let changed_folders = database::get_changed_folders_paths(&self.conn, drive_id)?;

        Ok(changed_folders
            .into_iter()
            .map(|(folder, path)| (folder, Path::from(path).path)))
    }

    #[tracing::instrument(skip(self))]
    pub fn get_changed_files_paths(
        &self,
        drive_id: &str,
    ) -> Result<impl Iterator<Item = (ChangedFile, PathBuf)>> {
        let changed_files = database::get_changed_files_paths(&self.conn, drive_id)?;

        Ok(changed_files
            .into_iter()
            .map(|(file, path)| (file, Path::from(path).path)))
    }
}

pub struct BernardBuilder {
    database_path: String,
    fetch: FetchBuilder,
}

impl BernardBuilder {
    pub fn new<S: Into<String>>(database_path: S, account: Account) -> Self {
        Self {
            database_path: database_path.into(),
            fetch: Fetcher::builder(account),
        }
    }

    pub async fn build(self) -> Result<Bernard> {
        let conn = database::establish_connection(&self.database_path)?;
        database::run_migration(&conn)?;

        Ok(Bernard {
            conn,
            fetch: Arc::new(self.fetch.build().await),
        })
    }

    pub fn proxy<U: IntoUrl>(mut self, url: U) -> Self {
        self.fetch = self.fetch.proxy(url);
        self
    }
}
