use crate::http::{
    HttpState,
    error::ApiError,
    extract::{ApiPath, Session},
    v1::files::{
        contracts::FileBytes,
        original_responses::{original_headers, parse_range_header, value},
    },
};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

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
    if file.state != application::files::FileState::Ready {
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
