use database::Pool;
use fetch::{FetchBuilder, Fetcher};
use jsonwebtoken::EncodingKey;
use reqwest::IntoUrl;
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

mod changes;
mod database;
mod fetch;
mod model;

pub use changes::Changes;
pub use model::{ChangedFile, ChangedFolder, ChangedPath, File, Folder, InnerPath, Path};

#[derive(Debug, Snafu)]
pub struct Error(InnerError);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Database,
    Network,
    PartialChangeList,
    WhereIsJWK,
    InvalidJWK,
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
enum InnerError {
    #[snafu(display("Database error: {}", source))]
    Database { source: sqlx::Error },
    #[snafu(display("Network error: {}", source))]
    Network { source: fetch::Error },
    #[snafu(display("Received a partial change list from Google. Database error: {}", source))]
    PartialChangeList { source: sqlx::Error },
    #[snafu(display("Cannot read the Service Account JWK file: {:?}. IO error: {}", file_name, source))]
    WhereIsJWK {
        file_name: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Invalid Service Account JWK file: {:?}. JSON error: {}", file_name, source))]
    InvalidJWK {
        file_name: PathBuf,
        source: serde_json::Error,
    },
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        use InnerError::*;

        match self.0 {
            Database { .. } => ErrorKind::Database,
            Network { .. } => ErrorKind::Network,
            PartialChangeList { .. } => ErrorKind::PartialChangeList,
            WhereIsJWK { .. } => ErrorKind::WhereIsJWK,
            InvalidJWK { .. } => ErrorKind::InvalidJWK,
        }
    }

    pub fn is_partial_change_list(&self) -> bool {
        matches!(self.0, InnerError::PartialChangeList { .. })
    }
}

impl From<sqlx::Error> for Error {
    fn from(source: sqlx::Error) -> Self {
        match &source {
            sqlx::Error::Database(db_err) => match db_err.code() {
                Some(code) => match code.as_ref() {
                    "787" => Self(InnerError::PartialChangeList { source }),
                    _ => Self(InnerError::Database { source }),
                },
                _ => Self(InnerError::Database { source }),
            },
            _ => Self(InnerError::Database { source }),
        }
    }
}

impl From<fetch::Error> for Error {
    fn from(source: fetch::Error) -> Self {
        Self(InnerError::Network { source })
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Bernard {
    fetch: Arc<Fetcher>,
    pool: Pool,
}

// TODO: Better names
pub enum SyncKind<'a> {
    Full,
    Partial(Changes<'a>),
}

impl Bernard {
    pub fn builder<S: Into<String>>(database_path: S, account: Account) -> BernardBuilder {
        BernardBuilder::new(database_path, account)
    }

    pub async fn close(self) {
        self.pool.close().await
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub async fn sync_drive<'a>(&'a self, drive_id: &'a str) -> Result<SyncKind<'a>> {
        // Always clear changelog for consistent database state when sync_drive is called.
        database::clear_changelog(drive_id, &self.pool).await?;

        let drive = database::get_drive(drive_id, &self.pool).await?;

        match drive {
            None => {
                info!("starting full synchronisation");
                let page_token = self.fetch.clone().start_page_token(drive_id).await?;

                // Might want to sleep between page_token and items
                let name = self.fetch.clone().drive_name(drive_id).await?;
                let items = self.fetch.clone().all_files(drive_id).await?;

                database::add_drive(drive_id, &name, &page_token, items, &self.pool).await?;

                Ok(SyncKind::Full)
            }
            Some(drive) => {
                info!("starting partial synchronisation");

                let (changes, new_page_token) = self
                    .fetch
                    .clone()
                    .changes(drive_id, &drive.page_token)
                    .await?;

                match new_page_token == drive.page_token {
                    // Do not perform database operation if no changes are available.
                    true => {
                        info!(page_token = %new_page_token, "page token has not changed");
                    }
                    false => {
                        info!(page_token = %new_page_token, "page token has changed");
                        database::merge_changes(drive_id, changes, &new_page_token, &self.pool)
                            .await?;
                    }
                };

                Ok(SyncKind::Partial(Changes::new(self, drive_id)))
            }
        }
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

    // Instead of build, simply call .await?
    pub async fn build(self) -> Result<Bernard> {
        let pool = database::establish_connection(&self.database_path).await?;

        Ok(Bernard {
            fetch: Arc::new(self.fetch.build()),
            pool,
        })
    }

    pub fn proxy<U: IntoUrl>(mut self, url: U) -> Self {
        self.fetch = self.fetch.proxy(url);
        self
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "String")]
struct PrivateKey(EncodingKey);

impl TryFrom<String> for PrivateKey {
    type Error = jsonwebtoken::errors::Error;

    fn try_from(key: String) -> std::result::Result<Self, Self::Error> {
        let key = EncodingKey::from_rsa_pem(key.as_ref())?;
        Ok(Self(key))
    }
}

#[derive(Debug, Deserialize)]
pub struct Account {
    client_email: String,
    private_key: PrivateKey,
}

impl Account {
    pub fn from_file<P: AsRef<std::path::Path>>(file_name: P) -> Result<Self> {
        let file = std::fs::File::open(file_name.as_ref()).context(WhereIsJWK {
            file_name: file_name.as_ref(),
        })?;

        let account: Self =
            serde_json::from_reader(std::io::BufReader::new(file)).context(InvalidJWK {
                file_name: file_name.as_ref(),
            })?;

        Ok(account)
    }
}

// Test whether the readme example contains valid code.
// Source: https://github.com/rust-lang/cargo/issues/383.
#[cfg(doctest)]
mod test_readme {
    macro_rules! external_doc_test {
        ($x:expr) => {
            #[doc = $x]
            extern "C" {}
        };
    }

    external_doc_test!(include_str!("../README.md"));
}
