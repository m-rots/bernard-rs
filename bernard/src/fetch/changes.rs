use super::{Change, Fetcher, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

impl Fetcher {
    pub async fn start_page_token(self: Arc<Fetcher>, drive_id: &str) -> Result<String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Query<'a> {
            drive_id: &'a str,
            fields: &'a str,
            supports_all_drives: bool,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            start_page_token: String,
        }

        let query = Query {
            drive_id,
            fields: "startPageToken",
            supports_all_drives: true,
        };

        let request = self
            .client
            .get("https://www.googleapis.com/drive/v3/changes/startPageToken")
            .query(&query);

        let Response { start_page_token } = self.make_request(request).await?;

        Ok(start_page_token)
    }
}

impl Fetcher {
    pub async fn changes(
        self: Arc<Fetcher>,
        drive_id: &str,
        page_token: &str,
    ) -> Result<(Vec<Change>, String)> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Query<'a> {
            drive_id: &'a str,
            page_token: &'a str,

            fields: &'a str,
            page_size: usize,

            #[serde(rename = "includeItemsFromAllDrives")]
            all_drives: bool,
            supports_all_drives: bool,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            changes: Vec<Change>,
            next_page_token: Option<String>,
            new_start_page_token: Option<String>,
        }

        let mut all_changes: Vec<Change> = Vec::new();
        let mut page_token = page_token.to_string();

        loop {
            let fetcher = self.clone();

            let query = Query {
                drive_id,
                page_token: &page_token,

                fields: "nextPageToken,newStartPageToken,changes(driveId,fileId,removed,drive(id,name),file(id,driveId,name,parents,md5Checksum,size,trashed))",
                page_size: 1000,

                all_drives: true,
                supports_all_drives: true,
            };

            let request = fetcher
                .client
                .get("https://www.googleapis.com/drive/v3/changes")
                .query(&query);

            let response: Response = fetcher.make_request(request).await?;

            all_changes.extend(response.changes);

            if let Some(next_page_token) = response.next_page_token {
                page_token = next_page_token;
            }

            if let Some(start_page_token) = response.new_start_page_token {
                return Ok((all_changes, start_page_token));
            }
        }
    }
}
