use anyhow::{Context, Result};
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

/// Manages temporary files created for complex command arguments.
#[derive(Debug, Clone)]
pub struct TempFileManager {
    temp_files: Arc<Mutex<Vec<NamedTempFile>>>,
}

impl TempFileManager {
    /// Creates a new temporary file manager.
    pub fn new() -> Self {
        Self {
            temp_files: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a temporary file with the given content and returns the file path.
    pub async fn create_temp_file_with_content(&self, content: &str) -> Result<String> {
        let mut temp_file = NamedTempFile::new()
            .context("Failed to create temporary file for multi-line argument")?;

        // Perform the blocking write on Tokio's blocking thread pool.
        // Note: spawn_blocking is appropriate here per R16.3 - the tempfile crate
        // only offers synchronous write APIs.
        let temp_file = {
            // Move the NamedTempFile into the blocking task and return it after write.
            let content = content.to_owned();
            tokio::task::spawn_blocking(move || -> Result<NamedTempFile> {
                temp_file
                    .write_all(content.as_bytes())
                    .context("Failed to write content to temporary file")?;
                temp_file
                    .flush()
                    .context("Failed to flush temporary file")?;
                Ok(temp_file)
            })
            .await
            .context("Failed to run blocking write in background")??
        };

        let file_path = temp_file.path().to_string_lossy().to_string();

        // Store the temp file so it doesn't get cleaned up until the manager is dropped
        {
            let mut temp_files = self.temp_files.lock().await;
            temp_files.push(temp_file);
        }

        Ok(file_path)
    }
}

impl Default for TempFileManager {
    fn default() -> Self {
        Self::new()
    }
}
