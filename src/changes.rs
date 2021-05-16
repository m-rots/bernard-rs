use crate::{database, Bernard, ChangedFile, ChangedFolder, ChangedPath, Result};

// Opportunity: Changes could hold the transaction to ensure it reflects the current database state.
// To make this work, the *actual* transaction would use a savepoint.
pub struct Changes<'a> {
    bernard: &'a Bernard,
    drive_id: &'a str,
}

impl<'a> Changes<'a> {
    pub(crate) fn new(bernard: &'a Bernard, drive_id: &'a str) -> Self {
        Self { bernard, drive_id }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(self.drive_id))]
    pub async fn paths(&self) -> Result<Vec<ChangedPath>> {
        database::get_changed_paths(self.drive_id, &self.bernard.pool)
            .await
            .map_err(|e| e.into())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn folders(&self) -> Result<Vec<ChangedFolder>> {
        database::get_changed_folders(self.drive_id, &self.bernard.pool)
            .await
            .map_err(|e| e.into())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn files(&self) -> Result<Vec<ChangedFile>> {
        database::get_changed_files(self.drive_id, &self.bernard.pool)
            .await
            .map_err(|e| e.into())
    }
}
