use crate::http::{
    HttpState,
    error::ApiError,
    extract::ApiJson,
    v1::identity::contracts::{AcceptedOutput, EmailInput},
};
use axum::{Extension, Json, extract::State, http::StatusCode, response::IntoResponse};
use jobs::TraceContext;

#[utoipa::path(post, path="/api/v1/auth/verification", tag="Authentication", request_body=EmailInput, responses((status=202,body=AcceptedOutput)))]
pub async fn request_verification(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<EmailInput>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .app
        .identity
        .request_verification(&input.email, trace)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AcceptedOutput { accepted: true }),
    ))
}
