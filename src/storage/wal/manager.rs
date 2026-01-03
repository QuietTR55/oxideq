use super::WalEntry;
use std::path::Path;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug)]
pub struct WalManager {
    file: File,
}

impl WalManager {
    pub async fn new(path: impl AsRef<Path>) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .await
            .expect("Failed to open WAL file");

        Self { file }
    }

    pub async fn append(&mut self, entry: &WalEntry) -> Result<(), String> {
        let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        line.push('\n');
        self.file
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.file.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn replay(path: impl AsRef<Path>) -> Result<Vec<WalEntry>, String> {
        let file = OpenOptions::new().read(true).open(path).await;

        match file {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut lines = reader.lines();
                let mut entries = Vec::new();

                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let entry: WalEntry = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                    entries.push(entry);
                }
                Ok(entries)
            }
            Err(_) => Ok(Vec::new()), // File doesn't exist yet, return empty
        }
    }
}
