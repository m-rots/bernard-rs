use super::{Fetcher, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

impl Fetcher {
    pub async fn drive_name(self: Arc<Fetcher>, drive_id: &str) -> Result<String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Query<'a> {
            fields: &'a str,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            name: String,
        }

        let query = Query { fields: "name" };

        let request = self
            .client
            .get(format!(
                "https://www.googleapis.com/drive/v3/drives/{}",
                drive_id
            ))
            .query(&query);

        let Response { name } = self.make_request(request).await?;

        Ok(name)
    }
}
