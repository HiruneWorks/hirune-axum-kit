use crate::http::{
    HttpState,
    error::ApiError,
    extract::ApiJson,
    v1::identity::{
        contracts::{LoginOutput, SessionTransport},
        session_responses::{login_response, validate_transport},
    },
    validation,
};
use axum::{Extension, extract::State, http::HeaderMap, response::Response};
use jobs::TraceContext;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginInput {
    #[validate(custom(function = "validation::email"))]
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub transport: SessionTransport,
}

#[utoipa::path(post, path="/api/v1/auth/login", tag="Authentication", request_body=LoginInput, responses((status=200,body=LoginOutput)))]
pub async fn login(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<LoginInput>,
) -> Result<Response, ApiError> {
    validate_transport(&state, &headers, input.transport)?;
    let outcome = state
        .app
        .identity
        .login_with_password(&input.email, input.password, trace)
        .await?;
    Ok(login_response(&state, outcome, input.transport))
}
