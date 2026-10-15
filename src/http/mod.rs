mod api_access;
mod api_access_contracts;
mod contracts;
mod cookies;
mod error;
mod extract;
mod files;
mod identity;
mod middleware;
mod oidc;
mod openapi;
mod organizations;
mod validation;

use crate::{bootstrap::Application, config::HttpSettings};
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, StatusCode, header},
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;

pub use openapi::document as openapi_document;

#[derive(Clone)]
pub struct HttpState {
    pub app: Application,
    pub settings: Arc<HttpSettings>,
    request_permits: Arc<Semaphore>,
}

pub fn build_router(app: Application, settings: HttpSettings) -> Router {
    let origins: Vec<HeaderValue> = settings
        .allowed_origins
        .iter()
        .map(|origin| origin.parse().expect("validated origin"))
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::RANGE,
            header::IF_RANGE,
        ])
        .expose_headers([
            header::HeaderName::from_static("x-request-id"),
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::CONTENT_DISPOSITION,
            header::ETAG,
        ]);
    let state = HttpState {
        app,
        request_permits: Arc::new(Semaphore::new(settings.max_concurrent_requests)),
        settings: Arc::new(settings),
    };
    Router::new()
        .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
        .route("/health/ready", get(ready))
        .route(
            "/openapi.json",
            get(|| async { axum::Json(openapi_document()) }),
        )
        .route("/api/v1/auth/signup", post(identity::signup))
        .route(
            "/api/v1/auth/verification",
            post(identity::request_verification),
        )
        .route(
            "/api/v1/auth/verification/confirm",
            post(identity::verify_email),
        )
        .route("/api/v1/auth/login", post(identity::login))
        .route("/api/v1/auth/mfa", post(identity::complete_mfa))
        .route("/api/v1/auth/password/reset", post(identity::request_reset))
        .route(
            "/api/v1/auth/password/reset/confirm",
            post(identity::reset_password),
        )
        .route(
            "/api/v1/account",
            get(identity::get_account).delete(identity::deactivate_account),
        )
        .route(
            "/api/v1/account/password",
            axum::routing::put(identity::change_password),
        )
        .route(
            "/api/v1/account/totp",
            post(identity::begin_totp).delete(identity::disable_totp),
        )
        .route("/api/v1/account/totp/confirm", post(identity::confirm_totp))
        .route(
            "/api/v1/account/recovery-codes",
            post(identity::regenerate_recovery_codes),
        )
        .route(
            "/api/v1/sessions",
            get(identity::list_sessions).delete(identity::revoke_all_sessions),
        )
        .route(
            "/api/v1/sessions/{session_id}",
            axum::routing::delete(identity::revoke_session),
        )
        .route(
            "/api/v1/sessions/current/rotate",
            post(identity::rotate_session),
        )
        .route(
            "/api/v1/organizations",
            get(organizations::list_organizations).post(organizations::create_organization),
        )
        .route(
            "/api/v1/organizations/{organization_id}",
            get(organizations::get_organization).put(organizations::rename_organization),
        )
        .route(
            "/api/v1/organizations/{organization_id}/api-clients",
            get(api_access::list_clients).post(api_access::create_client),
        )
        .route(
            "/api/v1/organizations/{organization_id}/api-clients/{client_id}",
            get(api_access::get_client)
                .put(api_access::update_client)
                .delete(api_access::revoke_client),
        )
        .route(
            "/api/v1/organizations/{organization_id}/api-clients/{client_id}/keys",
            get(api_access::list_keys).post(api_access::issue_key),
        )
        .route(
            "/api/v1/organizations/{organization_id}/api-clients/{client_id}/keys/{key_id}",
            axum::routing::delete(api_access::revoke_key),
        )
        .route(
            "/api/v1/external/organization",
            get(api_access::external_organization).head(|| async {
                (
                    StatusCode::METHOD_NOT_ALLOWED,
                    [(axum::http::header::ALLOW, "GET")],
                )
            }),
        )
        .route("/api/v1/auth/oidc/{provider}", post(oidc::begin_login))
        .route("/api/v1/auth/oidc/{provider}/callback", get(oidc::callback))
        .route("/api/v1/account/identities", get(oidc::list_identities))
        .route(
            "/api/v1/organizations/{organization_id}/files",
            get(files::list_files).post(files::reserve_file),
        )
        .route(
            "/api/v1/organizations/{organization_id}/files/search",
            get(files::search_files),
        )
        .route(
            "/api/v1/organizations/{organization_id}/files/{file_id}",
            get(files::get_file).delete(files::delete_file),
        )
        .route(
            "/api/v1/organizations/{organization_id}/files/{file_id}/content",
            get(files::download_original)
                .head(files::head_original)
                .put(files::upload_original),
        )
        .route(
            "/api/v1/account/identities/providers/{provider}",
            post(oidc::begin_link),
        )
        .route(
            "/api/v1/account/identities/{identity_id}",
            axum::routing::delete(oidc::unlink_identity),
        )
        .layer(DefaultBodyLimit::max(16_384))
        .layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::request_context,
        ))
        .layer(sentry::integrations::tower::NewSentryLayer::<
            axum::extract::Request,
        >::new_from_top())
        .with_state(state)
}

async fn ready(State(state): State<HttpState>) -> StatusCode {
    if state.app.is_ready().await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
