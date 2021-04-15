#[macro_use]
extern crate diesel;

use auth::Account;
use database::SqliteConnection;
use fetch::{FetchBuilder, Fetcher};
use model::Drive;
use reqwest::IntoUrl;
use snafu::Snafu;
use std::sync::Arc;
use tokio::task::block_in_place;

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
}

impl From<database::Error> for Error {
    fn from(source: database::Error) -> Self {
        Self::Database { source }
    }
}

impl From<fetch::Error> for Error {
    fn from(source: fetch::Error) -> Self {
        Self::Network { source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Bernard<'a> {
    conn: Arc<SqliteConnection>,
    fetch: Fetcher<'a>,
}

impl<'a> Bernard<'a> {
    pub fn builder(database_path: &'a str, account: &'a Account) -> BernardBuilder<'a> {
        BernardBuilder::new(database_path, account)
    }

    async fn fill_drive(&mut self, drive_id: &str) -> Result<()> {
        let items = self.fetch.all_files(drive_id).await?;
        block_in_place(|| database::add_content(&self.conn, items))?;

        Ok(())
    }

    async fn initialise_drive(&mut self, drive_id: &str) -> Result<()> {
        let page_token = self.fetch.start_page_token(drive_id).await?;
        let name = self.fetch.drive_name(drive_id).await?;

        block_in_place(|| database::add_drive(&self.conn, drive_id, &name, &page_token))?;

        Ok(())
    }

    pub async fn add_drive(&mut self, drive_id: &str) -> Result<()> {
        self.initialise_drive(drive_id).await?;
        self.fill_drive(drive_id).await?;
        block_in_place(|| database::clear_changelog(&self.conn, drive_id))?;

        Ok(())
    }

    pub async fn sync_drive(&mut self, drive_id: &str) -> Result<()> {
        let drive = block_in_place(|| -> Result<Option<Drive>> {
            let drive = database::get_drive(&self.conn, drive_id)?;
            database::clear_changelog(&self.conn, drive_id)?;

            Ok(drive)
        })?;

        match drive {
            None => {
                self.add_drive(drive_id).await?;
            }
            Some(drive) => {
                let (changes, page_token) = self.fetch.changes(drive_id, &drive.page_token).await?;

                block_in_place(|| {
                    database::merge_changes(&self.conn, drive_id, changes, &page_token)
                })?;
            }
        };

        Ok(())
    }

    pub fn remove_drive(&self, drive_id: &str) -> Result<()> {
        database::remove_drive(&self.conn, drive_id)?;
        Ok(())
    }

    pub fn get_changelog(&self, drive_id: &str) -> Result<(Vec<ChangedFolder>, Vec<ChangedFile>)> {
        let changed_folders = database::get_changed_folders(&self.conn, drive_id)?;
        let changed_files = database::get_changed_files(&self.conn, drive_id)?;
        Ok((changed_folders, changed_files))
    }

    pub fn get_changed_paths(&self, drive_id: &str) -> Result<Vec<ChangedPath>> {
        let changed_paths = database::get_changed_paths(&self.conn, drive_id)?;
        Ok(changed_paths)
    }
}

pub struct BernardBuilder<'a> {
    database_path: &'a str,
    fetch: FetchBuilder<'a>,
}

impl<'a> BernardBuilder<'a> {
    pub fn new(database_path: &'a str, account: &'a Account) -> Self {
        Self {
            database_path,
            fetch: Fetcher::builder(account),
        }
    }

    pub async fn build(self) -> Result<Bernard<'a>> {
        let conn = database::establish_connection(self.database_path)?;
        database::run_migration(&conn)?;

        Ok(Bernard {
            conn: Arc::new(conn),
            fetch: self.fetch.build().await,
        })
    }

    pub fn proxy<U: IntoUrl>(mut self, url: U) -> Self {
        self.fetch = self.fetch.proxy(url);
        self
    }
}
