use crate::http::{
    HttpState,
    error::ApiError,
    extract::{ApiPath, Session},
    v1::files::contracts::{FileBytes, FileOutput},
};
use axum::{
    Json,
    extract::{Request, State},
};
use futures_util::StreamExt;
use jobs::TraceContext;
use uuid::Uuid;

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
