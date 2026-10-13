use super::{
    HttpState,
    contracts::*,
    cookies,
    error::ApiError,
    extract::{ApiPath, ApiQuery, Session, read_cookie},
    identity::login_response,
    middleware::require_browser_origin,
};
use application::identity::oidc::CallbackOutcome;
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use jobs::TraceContext;
use uuid::Uuid;

#[utoipa::path(post, path="/api/v1/auth/oidc/{provider}", tag="OIDC", params(("provider"=String,Path)), responses((status=200,body=OidcRedirectOutput)))]
pub async fn begin_login(
    State(state): State<HttpState>,
    ApiPath(provider): ApiPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_browser_origin(&state, &headers)?;
    let binding = read_cookie(&headers, state.settings.oauth_cookie_name())?;
    let redirect = state
        .app
        .oidc
        .begin_authorization(&provider, None, binding.as_deref())
        .await?;
    Ok(redirect_response(&state, redirect))
}
#[utoipa::path(post, path="/api/v1/account/identities/providers/{provider}", tag="OIDC", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("provider"=String,Path)), responses((status=200,body=OidcRedirectOutput)))]
pub async fn begin_link(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(provider): ApiPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_browser_origin(&state, &headers)?;
    let binding = read_cookie(&headers, state.settings.oauth_cookie_name())?;
    let redirect = state
        .app
        .oidc
        .begin_authorization(&provider, Some(&actor), binding.as_deref())
        .await?;
    Ok(redirect_response(&state, redirect))
}
#[utoipa::path(get, path="/api/v1/auth/oidc/{provider}/callback", tag="OIDC", params(("provider"=String,Path),OidcCallbackQuery), responses((status=200,body=LoginOutput),(status=204)))]
pub async fn callback(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiPath(provider): ApiPath<String>,
    ApiQuery(query): ApiQuery<OidcCallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let binding = read_cookie(&headers, state.settings.oauth_cookie_name())?
        .ok_or(application::ApplicationError::Unauthenticated)?;
    if query.code.is_some() == query.error.is_some() {
        return Err(ApiError::invalid_input());
    }
    let outcome = state
        .app
        .oidc
        .complete_authorization(
            &provider,
            &query.state,
            &binding,
            query.code.as_deref(),
            query.iss.as_deref(),
            trace,
        )
        .await?;
    let mut response = match outcome {
        CallbackOutcome::Login(outcome) => {
            login_response(&state, outcome, SessionTransport::Cookie)
        }
        CallbackOutcome::Linked => StatusCode::NO_CONTENT.into_response(),
    };
    cookies::set_cookie(
        &mut response,
        &state.settings,
        state.settings.oauth_cookie_name(),
        String::new(),
        0,
    );
    Ok(response)
}
#[utoipa::path(get, path="/api/v1/account/identities", tag="OIDC", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=200,body=Vec<ExternalIdentityOutput>)))]
pub async fn list_identities(
    State(state): State<HttpState>,
    Session(actor): Session,
) -> Result<Json<Vec<ExternalIdentityOutput>>, ApiError> {
    Ok(Json(
        state
            .app
            .oidc
            .list_external_identities(&actor)
            .await?
            .into_iter()
            .map(|identity| ExternalIdentityOutput {
                id: identity.id,
                issuer: identity.issuer,
            })
            .collect(),
    ))
}
#[utoipa::path(delete, path="/api/v1/account/identities/{identity_id}", tag="OIDC", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("identity_id"=Uuid,Path)), responses((status=204)))]
pub async fn unlink_identity(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiPath(identity_id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .oidc
        .unlink_external_identity(&actor, identity_id, trace)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
fn redirect_response(
    state: &HttpState,
    redirect: application::identity::oidc::AuthorizationRedirect,
) -> Response {
    let mut response = Json(OidcRedirectOutput {
        authorization_url: redirect.url.into(),
    })
    .into_response();
    cookies::set_cookie(
        &mut response,
        &state.settings,
        state.settings.oauth_cookie_name(),
        redirect.browser_binding,
        300,
    );
    response
}
