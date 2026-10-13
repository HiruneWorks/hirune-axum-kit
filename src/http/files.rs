use super::{
    HttpState,
    contracts::PaginationQuery,
    error::ApiError,
    extract::{ApiJson, ApiPath, ApiQuery, Session},
};
use application::files::{FileSummary, ReserveFile};
use axum::{
    Json,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use jobs::TraceContext;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReserveFileInput {
    pub filename: String,
    pub media_type: Option<String>,
    pub byte_size: u64,
}

#[derive(Serialize, ToSchema)]
pub struct FileOutput {
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

impl From<FileSummary> for FileOutput {
    fn from(file: FileSummary) -> Self {
        Self {
            id: file.id,
            organization_id: file.organization_id,
            uploader_id: file.uploader_id,
            filename: file.filename,
            media_type: file.media_type,
            byte_size: file.byte_size,
            state: file.state,
            version: file.version,
            content_hash: file.content_hash,
            upload_expires_at: file.upload_expires_at,
            created_at: file.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct FilePageOutput {
    pub items: Vec<FileOutput>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct FileSearchOutput {
    pub items: Vec<FileOutput>,
}
#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct FileSearchQuery {
    pub q: String,
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

#[utoipa::path(post, path="/api/v1/organizations/{organization_id}/files", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path)), request_body=ReserveFileInput, responses((status=201,body=FileOutput)))]
pub async fn reserve_file(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    axum::Extension(trace): axum::Extension<TraceContext>,
    ApiJson(input): ApiJson<ReserveFileInput>,
) -> Result<(StatusCode, Json<FileOutput>), ApiError> {
    let file = state
        .app
        .files
        .reserve_file(
            &actor,
            organization_id,
            ReserveFile {
                filename: input.filename,
                media_type: input.media_type,
                byte_size: input.byte_size,
            },
            trace,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(file.into())))
}

#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/files", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path), PaginationQuery), responses((status=200,body=FilePageOutput)))]
pub async fn list_files(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<PaginationQuery>,
) -> Result<Json<FilePageOutput>, ApiError> {
    let page = state
        .app
        .files
        .list_files(
            &actor,
            organization_id,
            query.limit,
            query.cursor.as_deref(),
        )
        .await?;
    Ok(Json(FilePageOutput {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/files/{file_id}", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("file_id"=Uuid,Path)), responses((status=200,body=FileOutput)))]
pub async fn get_file(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, file_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<FileOutput>, ApiError> {
    Ok(Json(
        state
            .app
            .files
            .get_file(&actor, organization_id, file_id)
            .await?
            .into(),
    ))
}

#[utoipa::path(put, path="/api/v1/organizations/{organization_id}/files/{file_id}/content", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("file_id"=Uuid,Path)), request_body(content=FileBytes,content_type="application/octet-stream"), responses((status=200,body=FileOutput)))]
pub async fn upload_original(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, file_id)): ApiPath<(Uuid, Uuid)>,
    axum::Extension(trace): axum::Extension<TraceContext>,
    request: Request,
) -> Result<Json<FileOutput>, ApiError> {
    let body = request
        .into_body()
        .into_data_stream()
        .map(|chunk| chunk.map_err(|_| std::io::Error::other("upload body interrupted")))
        .boxed();
    Ok(Json(
        state
            .app
            .files
            .upload_original(&actor, organization_id, file_id, body, trace)
            .await?
            .into(),
    ))
}

#[utoipa::path(delete, path="/api/v1/organizations/{organization_id}/files/{file_id}", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("file_id"=Uuid,Path)), responses((status=204)))]
pub async fn delete_file(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, file_id)): ApiPath<(Uuid, Uuid)>,
    axum::Extension(trace): axum::Extension<TraceContext>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .files
        .delete_file(&actor, organization_id, file_id, trace)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/files/search", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),FileSearchQuery), responses((status=200,body=FileSearchOutput)))]
pub async fn search_files(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<FileSearchQuery>,
) -> Result<Json<FileSearchOutput>, ApiError> {
    Ok(Json(FileSearchOutput {
        items: state
            .app
            .files
            .search_files(&actor, organization_id, &query.q)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    }))
}

#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/files/{file_id}/content", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("file_id"=Uuid,Path),("Range"=Option<String>,Header),("If-Range"=Option<String>,Header)), responses((status=200,body=FileBytes,content_type="application/octet-stream"),(status=206,body=FileBytes,content_type="application/octet-stream"),(status=416,description="Unsatisfiable or unsupported byte range")))]
pub async fn download_original(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, file_id)): ApiPath<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let file = state
        .app
        .files
        .get_file(&actor, organization_id, file_id)
        .await?;
    if file.state != "ready" {
        return Err(application::ApplicationError::NotFound.into());
    }
    let etag = file.content_hash.as_ref().map(|hash| format!("\"{hash}\""));
    let range = parse_range_header(&headers, file.byte_size, etag.as_deref());
    let range = match range {
        Ok(range) => range,
        Err(()) => {
            let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                value(&format!("bytes */{}", file.byte_size))?,
            );
            return Ok(response);
        }
    };
    let content = state
        .app
        .files
        .download_original(&actor, organization_id, file_id, range.clone())
        .await?;
    let mut response = Response::new(Body::from_stream(content.stream));
    *response.status_mut() = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    original_headers(response.headers_mut(), &content.file, content.byte_length)?;
    if let Some(range) = range {
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            value(&format!(
                "bytes {}-{}/{}",
                range.start,
                range.end - 1,
                content.file.byte_size
            ))?,
        );
    }
    Ok(response)
}

#[utoipa::path(head, path="/api/v1/organizations/{organization_id}/files/{file_id}/content", tag="Files", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("file_id"=Uuid,Path)), responses((status=200)))]
pub async fn head_original(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, file_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    let file = state
        .app
        .files
        .original_metadata(&actor, organization_id, file_id)
        .await?;
    let mut response = Response::new(Body::empty());
    original_headers(response.headers_mut(), &file, file.byte_size)?;
    Ok(response)
}

fn original_headers(
    headers: &mut HeaderMap,
    file: &FileSummary,
    byte_length: u64,
) -> Result<(), ApiError> {
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, value(&byte_length.to_string())?);
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let filename =
        percent_encoding::utf8_percent_encode(&file.filename, percent_encoding::NON_ALPHANUMERIC);
    headers.insert(
        header::CONTENT_DISPOSITION,
        value(&format!(
            "attachment; filename=\"attachment\"; filename*=UTF-8''{filename}"
        ))?,
    );
    if let Some(hash) = &file.content_hash {
        headers.insert(header::ETAG, value(&format!("\"{hash}\""))?);
    }
    Ok(())
}

fn value(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|_| {
        application::ApplicationError::internal(anyhow::anyhow!("invalid file response header"))
            .into()
    })
}

fn parse_range_header(
    headers: &HeaderMap,
    size: u64,
    etag: Option<&str>,
) -> Result<Option<Range<u64>>, ()> {
    let mut ranges = headers.get_all(header::RANGE).iter();
    let Some(range) = ranges.next() else {
        return Ok(None);
    };
    if ranges.next().is_some() {
        return Err(());
    }
    if let Some(if_range) = headers.get(header::IF_RANGE)
        && (etag.is_none() || if_range.to_str().ok() != etag)
    {
        return Ok(None);
    }
    let range = range
        .to_str()
        .map_err(|_| ())?
        .strip_prefix("bytes=")
        .ok_or(())?;
    if range.contains(',') || size == 0 {
        return Err(());
    }
    let (start, end) = range.split_once('-').ok_or(())?;
    let number = |value: &str| -> Result<u64, ()> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(());
        }
        value.parse().map_err(|_| ())
    };
    let range = if start.is_empty() {
        let suffix = number(end)?;
        if suffix == 0 {
            return Err(());
        }
        size.saturating_sub(suffix)..size
    } else {
        let start = number(start)?;
        let end = if end.is_empty() {
            size
        } else {
            number(end)?.saturating_add(1).min(size)
        };
        if start >= end || start >= size {
            return Err(());
        }
        start..end
    };
    Ok(Some(range))
}

use axum::response::IntoResponse;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_ranges_cover_suffix_resume_and_unsatisfiable_requests() {
        for (input, expected) in [
            ("bytes=2-5", Some(2..6)),
            ("bytes=2-", Some(2..10)),
            ("bytes=-3", Some(7..10)),
            ("bytes=-99", Some(0..10)),
        ] {
            let headers = [(header::RANGE, value(input).unwrap())]
                .into_iter()
                .collect();
            assert_eq!(parse_range_header(&headers, 10, None), Ok(expected));
        }
        for input in [
            "bytes=10-",
            "bytes=3-2",
            "bytes=-0",
            "bytes=0-1,3-4",
            "bytes=+1-2",
            "items=0-1",
        ] {
            let headers = [(header::RANGE, value(input).unwrap())]
                .into_iter()
                .collect();
            assert!(parse_range_header(&headers, 10, None).is_err());
        }
        let headers = [
            (header::RANGE, value("bytes=0-1").unwrap()),
            (header::IF_RANGE, value("\"old\"").unwrap()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            parse_range_header(&headers, 10, Some("\"current\"")),
            Ok(None)
        );
    }
}
