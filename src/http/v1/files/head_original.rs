use crate::http::{
    HttpState,
    error::ApiError,
    extract::{ApiPath, Session},
    v1::files::original_responses::original_headers,
};
use axum::{body::Body, extract::State, response::Response};
use uuid::Uuid;

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
