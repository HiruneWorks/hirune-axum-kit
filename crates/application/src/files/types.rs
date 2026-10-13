use super::{S3Settings, SearchSettings, entities::file};
use crate::{ApplicationError, ApplicationResult};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::stream::BoxStream;
use std::path::PathBuf;
use uuid::Uuid;

pub type TransferStream = BoxStream<'static, Result<Bytes, std::io::Error>>;

#[derive(Clone, Debug)]
pub struct FileSettings {
    pub storage: Option<S3Settings>,
    pub search: Option<SearchSettings>,
    pub max_file_bytes: u64,
    pub organization_quota_bytes: u64,
    pub organization_file_limit: u64,
    pub pending_file_limit: u64,
    pub reservations_per_minute: u64,
    pub tombstone_days: i64,
    pub transfer_concurrency: usize,
    pub transfer_seconds: u64,
    pub idle_seconds: u64,
    pub temporary_directory: PathBuf,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            storage: None,
            search: None,
            max_file_bytes: 100 * 1024 * 1024,
            organization_quota_bytes: 1024 * 1024 * 1024,
            organization_file_limit: 10_000,
            pending_file_limit: 100,
            reservations_per_minute: 30,
            tombstone_days: 30,
            transfer_concurrency: 4,
            transfer_seconds: 300,
            idle_seconds: 30,
            temporary_directory: std::env::temp_dir(),
        }
    }
}

impl FileSettings {
    pub fn validate(&self) -> ApplicationResult<()> {
        if !(1..=1024 * 1024 * 1024).contains(&self.max_file_bytes)
            || self.organization_quota_bytes < self.max_file_bytes
            || self.organization_quota_bytes > i64::MAX as u64
            || !(1..=1_000_000).contains(&self.organization_file_limit)
            || !(1..=self.organization_file_limit).contains(&self.pending_file_limit)
            || !(1..=10_000).contains(&self.reservations_per_minute)
            || !(1..=3650).contains(&self.tombstone_days)
            || !(1..=64).contains(&self.transfer_concurrency)
            || !(30..=3600).contains(&self.transfer_seconds)
            || !(1..=self.transfer_seconds).contains(&self.idle_seconds)
            || !self.temporary_directory.is_absolute()
        {
            return Err(ApplicationError::InvalidInput("invalid file settings"));
        }
        if let Some(storage) = &self.storage {
            storage.validate()?;
        }
        if let Some(search) = &self.search {
            search.validate()?;
        }
        Ok(())
    }
}

pub struct ReserveFile {
    pub filename: String,
    pub media_type: Option<String>,
    pub byte_size: u64,
}

pub struct FileSummary {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub uploader_id: Uuid,
    pub filename: String,
    pub media_type: Option<String>,
    pub byte_size: u64,
    pub state: String,
    pub version: i64,
    pub content_hash: Option<String>,
    pub upload_expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<file::Model> for FileSummary {
    fn from(file: file::Model) -> Self {
        Self {
            id: file.id,
            organization_id: file.organization_id,
            uploader_id: file.uploader_id,
            filename: file.filename,
            media_type: file.media_type,
            byte_size: file.byte_size as u64,
            state: file.state,
            version: file.version,
            content_hash: file.content_hash,
            upload_expires_at: file.upload_expires_at,
            created_at: file.created_at,
        }
    }
}

pub struct FileContent {
    pub file: FileSummary,
    pub stream: TransferStream,
    pub byte_length: u64,
}
