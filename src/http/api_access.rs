use super::{
    HttpState,
    api_access_contracts::*,
    contracts::PaginationQuery,
    error::ApiError,
    extract::{ApiJson, ApiPath, ApiQuery, Session, read_bearer_token, read_cookie},
};
use application::{ApplicationError, config::SecretString};
use axum::{
    Extension, Json,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, request::Parts},
};
use jobs::TraceContext;
use uuid::Uuid;

pub struct ApiKeyCredential(SecretString);
impl FromRequestParts<HttpState> for ApiKeyCredential {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &HttpState,
    ) -> Result<Self, Self::Rejection> {
        if read_cookie(&parts.headers, state.settings.session_cookie_name())?.is_some() {
            return Err(ApiError::invalid_input());
        }
        let value = read_bearer_token(&parts.headers)?
            .filter(|value| value.len() <= 128)
            .ok_or(ApplicationError::Unauthenticated)?;
        Ok(Self(SecretString::new(value)))
    }
}

#[utoipa::path(post, path="/api/v1/organizations/{organization_id}/api-clients", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path)), request_body=ApiClientInput, responses((status=201,body=ApiClientOutput)))]
pub async fn create_client(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<ApiClientInput>,
) -> Result<(StatusCode, Json<ApiClientOutput>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .app
                .api_access
                .create_client(&actor, organization_id, input.name, input.scopes, trace)
                .await?
                .into(),
        ),
    ))
}
#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/api-clients", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path), PaginationQuery), responses((status=200,body=ApiClientPageOutput)))]
pub async fn list_clients(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<PaginationQuery>,
) -> Result<Json<ApiClientPageOutput>, ApiError> {
    let page = state
        .app
        .api_access
        .list_clients(
            &actor,
            organization_id,
            query.limit,
            query.cursor.as_deref(),
        )
        .await?;
    Ok(Json(ApiClientPageOutput {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}
#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path)), responses((status=200,body=ApiClientOutput)))]
pub async fn get_client(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<Json<ApiClientOutput>, ApiError> {
    Ok(Json(
        state
            .app
            .api_access
            .get_client(&actor, organization_id, client_id)
            .await?
            .into(),
    ))
}
#[utoipa::path(put, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path)), request_body=ApiClientInput, responses((status=200,body=ApiClientOutput)))]
pub async fn update_client(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id)): ApiPath<(Uuid, Uuid)>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<ApiClientInput>,
) -> Result<Json<ApiClientOutput>, ApiError> {
    Ok(Json(
        state
            .app
            .api_access
            .update_client(
                &actor,
                organization_id,
                client_id,
                input.name,
                input.scopes,
                trace,
            )
            .await?
            .into(),
    ))
}
#[utoipa::path(delete, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path)), responses((status=204)))]
pub async fn revoke_client(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id)): ApiPath<(Uuid, Uuid)>,
    Extension(trace): Extension<TraceContext>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .api_access
        .revoke_client(&actor, organization_id, client_id, trace)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}/keys", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path)), request_body=IssueApiKeyInput, responses((status=201,body=IssuedApiKeyOutput)))]
pub async fn issue_key(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id)): ApiPath<(Uuid, Uuid)>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<IssueApiKeyInput>,
) -> Result<(StatusCode, Json<IssuedApiKeyOutput>), ApiError> {
    let issued = state
        .app
        .api_access
        .issue_key(
            &actor,
            organization_id,
            client_id,
            input.name,
            input.expires_in_days,
            trace,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedApiKeyOutput {
            key: issued.key.into(),
            credential: issued.credential.expose().into(),
        }),
    ))
}
#[utoipa::path(get, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}/keys", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path), PaginationQuery), responses((status=200,body=ApiKeyPageOutput)))]
pub async fn list_keys(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id)): ApiPath<(Uuid, Uuid)>,
    ApiQuery(query): ApiQuery<PaginationQuery>,
) -> Result<Json<ApiKeyPageOutput>, ApiError> {
    let page = state
        .app
        .api_access
        .list_keys(
            &actor,
            organization_id,
            client_id,
            query.limit,
            query.cursor.as_deref(),
        )
        .await?;
    Ok(Json(ApiKeyPageOutput {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}
#[utoipa::path(delete, path="/api/v1/organizations/{organization_id}/api-clients/{client_id}/keys/{key_id}", tag="API access", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path),("client_id"=Uuid,Path),("key_id"=Uuid,Path)), responses((status=204)))]
pub async fn revoke_key(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath((organization_id, client_id, key_id)): ApiPath<(Uuid, Uuid, Uuid)>,
    Extension(trace): Extension<TraceContext>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .api_access
        .revoke_key(&actor, organization_id, client_id, key_id, trace)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path="/api/v1/external/organization", tag="External API", security(("apiKeyAuth"=[])), responses((status=200,body=ExternalOrganizationOutput, headers(
    ("X-RateLimit-Limit"=i64, description="Per-client fixed UTC minute allowance"),
    ("X-RateLimit-Remaining"=i64, description="Remaining minute requests"),
    ("X-RateLimit-Reset"=i64, description="Minute reset Unix timestamp"),
    ("X-Quota-Limit"=i64, description="Per-client UTC calendar-month allowance"),
    ("X-Quota-Remaining"=i64, description="Remaining monthly requests"),
    ("X-Quota-Reset"=i64, description="Month reset Unix timestamp")
))))]
pub async fn external_organization(
    State(state): State<HttpState>,
    credential: ApiKeyCredential,
    Extension(trace): Extension<TraceContext>,
) -> Result<(HeaderMap, Json<ExternalOrganizationOutput>), ApiError> {
    let result = state
        .app
        .api_access
        .read_organization(credential.0.expose(), trace)
        .await?;
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("x-ratelimit-limit", result.quota.minute_limit),
        (
            "x-ratelimit-remaining",
            result.quota.minute_limit - result.quota.minute_used,
        ),
        (
            "x-ratelimit-reset",
            result.quota.minute_resets_at.timestamp(),
        ),
        ("x-quota-limit", result.quota.month_limit),
        (
            "x-quota-remaining",
            result.quota.month_limit - result.quota.month_used,
        ),
        ("x-quota-reset", result.quota.month_resets_at.timestamp()),
    ] {
        headers.insert(
            name,
            HeaderValue::from_str(&value.to_string()).expect("integer header"),
        );
    }
    Ok((
        headers,
        Json(ExternalOrganizationOutput {
            id: result.id,
            name: result.name,
            version: result.version,
        }),
    ))
}
