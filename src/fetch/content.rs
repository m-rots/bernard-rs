use super::{Fetcher, Item, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

impl Fetcher {
    pub async fn all_files(self: Arc<Fetcher>, drive_id: &str) -> Result<Vec<Item>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Query<'a> {
            drive_id: &'a str,
            page_token: Option<String>,

            fields: &'a str,
            page_size: usize,

            corpora: &'a str,
            #[serde(rename = "includeItemsFromAllDrives")]
            all_drives: bool,
            supports_all_drives: bool,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            #[serde(rename = "files")]
            items: Vec<Item>,
            next_page_token: Option<String>,
        }

        let mut all_items: Vec<Item> = Vec::new();
        let mut page_token = None;

        loop {
            let fetch = self.clone();

            let query = Query {
                drive_id,
                page_token,

                fields: "nextPageToken,files(id,driveId,name,parents,md5Checksum,size,trashed)",
                page_size: 1000,

                corpora: "drive",
                all_drives: true,
                supports_all_drives: true,
            };

            let request = fetch
                .client
                .get("https://www.googleapis.com/drive/v3/files")
                .query(&query);

            let response: Response = fetch.with_retry(request).await?;

            all_items.extend(response.items);
            page_token = response.next_page_token;

            if page_token.is_none() {
                return Ok(all_items);
            }
        }
    }
}
