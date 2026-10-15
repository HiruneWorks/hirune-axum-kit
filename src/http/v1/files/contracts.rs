use application::files::FileSummary;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct FileOutput {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub uploader_id: Uuid,
    pub filename: String,
    pub media_type: Option<String>,
    pub byte_size: u64,
    pub state: FileState,
    pub version: i64,
    pub content_hash: Option<String>,
    pub upload_expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<FileSummary> for FileOutput {
    fn from(file: FileSummary) -> Self {
        Self {
            id: file.id,
            organization_id: file.organization_id,
            uploader_id: file.uploader_id,
            filename: file.filename,
            media_type: file.media_type,
            byte_size: file.byte_size,
            state: file.state.into(),
            version: file.version,
            content_hash: file.content_hash,
            upload_expires_at: file.upload_expires_at,
            created_at: file.created_at,
        }
    }
}

pub struct FileBytes;
impl utoipa::PartialSchema for FileBytes {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, SchemaFormat, Type};
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
            .build()
            .into()
    }
}
impl ToSchema for FileBytes {}

#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Uploading,
    Transferring,
    Ready,
    Deleting,
    Deleted,
}
impl From<application::files::FileState> for FileState {
    fn from(value: application::files::FileState) -> Self {
        match value {
            application::files::FileState::Uploading => Self::Uploading,
            application::files::FileState::Transferring => Self::Transferring,
            application::files::FileState::Ready => Self::Ready,
            application::files::FileState::Deleting => Self::Deleting,
            application::files::FileState::Deleted => Self::Deleted,
        }
    }
}
impl From<FileState> for application::files::FileState {
    fn from(value: FileState) -> Self {
        match value {
            FileState::Uploading => Self::Uploading,
            FileState::Transferring => Self::Transferring,
            FileState::Ready => Self::Ready,
            FileState::Deleting => Self::Deleting,
            FileState::Deleted => Self::Deleted,
        }
    }
}
