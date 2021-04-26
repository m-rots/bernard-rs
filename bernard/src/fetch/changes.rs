use super::{Change, Fetcher, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
            let fetch = self.clone();

            let query = Query {
                drive_id,
                page_token: &page_token,

                fields: "nextPageToken,newStartPageToken,changes(driveId,fileId,removed,drive(id,name),file(id,driveId,name,parents,md5Checksum,size,trashed))",
                page_size: 1000,

                all_drives: true,
                supports_all_drives: true,
            };

            let request = fetch
                .client
                .get("https://www.googleapis.com/drive/v3/changes")
                .query(&query);

            let response: Response = fetch.with_retry(request).await?;

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
