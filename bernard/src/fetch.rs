use crate::model::{File, Folder};
use crate::Account;
use auth::{AccessToken, RefreshToken, Scope};
use chrono::Duration;
use reqwest::{Client, ClientBuilder, IntoUrl, StatusCode};
use serde::de::Deserializer;
use serde::Deserialize;
use snafu::{Backtrace, ResultExt, Snafu};
use std::sync::Arc;
use tap::prelude::*;
use tracing::{debug, error, warn};
use tracing_futures::Instrument;

mod auth;
mod changes;
mod content;
mod drive;
mod page_token;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Google Drive API is not enabled"))]
    ApiNotEnabled { backtrace: Backtrace },
    #[snafu(display("Service Account does not have viewer permission on Shared Drive"))]
    DriveNotFound { backtrace: Backtrace },
    #[snafu(display("Unable to connect to the Google Drive API"))]
    ConnectionError { source: reqwest::Error },
    #[snafu(display("Unable to parse/deserialise the JSON response"))]
    DeserialisationError { source: reqwest::Error },
    #[snafu(display("Invalid Service Account Credentials"))]
    InvalidCredentials { backtrace: Backtrace },
    #[snafu(display("An unknown error occured!"))]
    UnknownStatus { status: StatusCode },
    #[snafu(display("The Google Drive API is having some issues"))]
    ServerError { status: StatusCode },
}

pub type Result<T> = std::result::Result<T, Error>;

fn to_backoff_error(error: Error) -> backoff::Error<Error> {
    match error {
        Error::ConnectionError { .. } | Error::ServerError { .. } => {
            backoff::Error::Transient(error)
        }
        _ => backoff::Error::Permanent(error),
    }
}

pub struct Fetcher {
    account: Account,
    client: Client,
    refresh_token: RefreshToken,
}

impl Fetcher {
    pub fn new(client: Client, account: Account) -> Fetcher {
        let scope = Scope::builder()
            .scope("https://www.googleapis.com/auth/drive.readonly")
            .lifetime(Duration::hours(1))
            .build();

        let refresh_token = RefreshToken::new(scope);

        Self {
            client,
            account,
            refresh_token,
        }
    }

    pub fn builder(account: Account) -> FetchBuilder {
        FetchBuilder::new(account)
    }

    async fn with_auth<T>(self: Arc<Fetcher>, request: reqwest::RequestBuilder) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let AccessToken { token, .. } = self.refresh_token.access_token(self.clone()).await?;

        let request = request.bearer_auth(token).build().unwrap();

        self.make_request_inner(request).await
    }

    async fn make_request_inner<T>(self: Arc<Fetcher>, request: reqwest::Request) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        debug!(url_path = %request.url().path(), "making request");

        let response = self
            .client
            .execute(request)
            .await
            .context(ConnectionError)?;

        let status = response.status();
        if status.is_success() {
            let response: T = response.json().await.context(DeserialisationError)?;
            return Ok(response);
        }

        if status.is_server_error() {
            return Err(ServerError { status }.build());
        }

        let error = match status {
            StatusCode::NOT_FOUND => DriveNotFound.build(),
            StatusCode::FORBIDDEN => {
                // TODO: Decode request to figure out whether it is a rate limit error
                ApiNotEnabled.build()
            }
            StatusCode::UNAUTHORIZED => InvalidCredentials.build(),
            _ => Error::UnknownStatus { status },
        };

        Err(error)
    }

    async fn with_retry<T>(self: Arc<Fetcher>, request: reqwest::RequestBuilder) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Send + 'static,
    {
        let future = async move {
            let response: T =
                backoff::future::retry(backoff::ExponentialBackoff::default(), || {
                    let request = request.try_clone().expect("Could not clone request");

                    async {
                        let response: T = self
                            .clone()
                            .with_auth(request)
                            .await
                            .map_err(to_backoff_error)
                            .tap_err(|error| match error {
                                backoff::Error::Permanent(error) => {
                                    error!(%error, "non-retryable error occured")
                                }
                                backoff::Error::Transient(error) => {
                                    warn!(%error, "retryable error occured")
                                }
                            })?;

                        Ok(response)
                    }
                })
                .await?;

            Ok(response)
        };

        tokio::spawn(future.in_current_span()).await.unwrap()
    }
}

pub struct FetchBuilder {
    account: Account,
    client: ClientBuilder,
}

impl FetchBuilder {
    pub fn new(account: Account) -> Self {
        Self {
            client: ClientBuilder::new(),
            account,
        }
    }

    pub fn build(self) -> Fetcher {
        let client = self.client.build().unwrap();

        Fetcher::new(client, self.account)
    }

    pub fn proxy<U: IntoUrl>(mut self, url: U) -> Self {
        let proxy = reqwest::Proxy::all(url).unwrap();

        self.client = self.client.proxy(proxy);
        self
    }
}

#[derive(Debug)]
pub enum Item {
    File(File),
    Folder(Folder),
}

impl Item {
    pub fn drive_id<'a>(&'a self) -> &'a str {
        match self {
            Item::File(file) => &file.drive_id,
            Item::Folder(folder) => &folder.drive_id,
        }
    }

    pub fn into_id(self) -> String {
        match self {
            Item::File(file) => file.id,
            Item::Folder(folder) => folder.id,
        }
    }

    pub fn id<'a>(&'a self) -> &'a str {
        match self {
            Item::File(file) => &file.id,
            Item::Folder(folder) => &folder.id,
        }
    }
}

// Custom deserializer for Item to parse into the correct enum variant.
impl<'de> Deserialize<'de> for Item {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Mapping {
            id: String,
            drive_id: String,
            md5_checksum: Option<String>,
            name: String,
            #[serde(deserialize_with = "from_vec", rename = "parents")]
            parent: Option<String>,
            size: Option<String>,
            trashed: bool,
        }

        let Mapping {
            id,
            drive_id,
            md5_checksum,
            name,
            parent,
            size,
            trashed,
        } = Mapping::deserialize(deserializer)?;

        match (md5_checksum, size, parent) {
            (Some(md5), Some(size), Some(parent)) => Ok(Self::File(File {
                id,
                drive_id,
                md5,
                name,
                parent,
                size: size.parse().map_err(D::Error::custom)?,
                trashed,
            })),
            (_, _, parent) => Ok(Self::Folder(Folder {
                id,
                drive_id,
                name,
                parent,
                trashed,
            })),
        }
    }
}

#[derive(Debug)]
pub enum Change {
    DriveChanged(PartialDrive),
    DriveRemoved(String),
    ItemChanged(Item),
    ItemRemoved(String),
}

#[derive(Debug, Deserialize)]
pub struct PartialDrive {
    pub id: String,
    pub name: String,
}

// Custom deserializer for Change to parse into the correct enum variant.
impl<'de> Deserialize<'de> for Change {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Mapping {
            #[serde(rename = "fileId")]
            item_id: Option<String>,
            #[serde(rename = "file")]
            item: Option<Item>,
            drive_id: Option<String>,
            drive: Option<PartialDrive>,
            removed: bool,
        }

        let Mapping {
            drive,
            drive_id,
            item,
            item_id,
            removed,
        } = Mapping::deserialize(deserializer)?;

        match (removed, drive, drive_id, item, item_id) {
            (true, None, Some(drive_id), None, None) => Ok(Self::DriveRemoved(drive_id)),
            (false, Some(drive), _, None, None) => Ok(Self::DriveChanged(drive)),
            (true, None, None, None, Some(item_id)) => Ok(Self::ItemRemoved(item_id)),
            (false, None, None, Some(item), _) => Ok(Self::ItemChanged(item)),
            _ => Err(D::Error::custom("unknown change variant")),
        }
    }
}

/// Convert a `Vec<String>` into an `Option<String>` with the first element of the vec.
fn from_vec<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let parents: Vec<String> = Deserialize::deserialize(deserializer)?;
    Ok(parents.into_iter().next())
}
