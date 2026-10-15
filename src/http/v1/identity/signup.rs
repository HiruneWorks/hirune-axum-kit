use crate::http::{
    HttpState, error::ApiError, extract::ApiJson, v1::identity::contracts::AcceptedOutput,
    validation,
};
use axum::{Extension, Json, extract::State, http::StatusCode, response::IntoResponse};
use jobs::TraceContext;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterInput {
    #[validate(custom(function = "validation::email"))]
    pub email: String,
    #[validate(custom(function = "validation::new_password"))]
    /// At least 15 Unicode characters and at most 1024 UTF-8 bytes.
    #[schema(min_length = 15)]
    pub password: String,
}

#[utoipa::path(post, path="/api/v1/auth/signup", tag="Authentication", request_body=RegisterInput, responses((status=202,body=AcceptedOutput)))]
pub async fn signup(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<RegisterInput>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .app
        .identity
        .register_account(&input.email, input.password, trace)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AcceptedOutput { accepted: true }),
    ))
}
