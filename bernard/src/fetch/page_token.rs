use super::{Fetcher, Result};
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

        let Response { start_page_token } = self.with_retry(request).await?;

        Ok(start_page_token)
    }
}
