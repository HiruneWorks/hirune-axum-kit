use super::{
    HttpState,
    contracts::*,
    cookies,
    error::ApiError,
    extract::{ApiJson, ApiPath, ApiQuery, Session},
    middleware::require_browser_origin,
};
use application::identity::{LoginOutcome, SessionCredentials};
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use jobs::TraceContext;
use uuid::Uuid;

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
#[utoipa::path(post, path="/api/v1/auth/verification/confirm", tag="Authentication", request_body=TokenInput, responses((status=204)))]
pub async fn verify_email(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<TokenInput>,
) -> Result<StatusCode, ApiError> {
    state.app.identity.verify_email(&input.token, trace).await?;
    Ok(StatusCode::NO_CONTENT)
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
#[utoipa::path(post, path="/api/v1/auth/mfa", tag="Authentication", request_body=MfaLoginInput, responses((status=200,body=LoginOutput)))]
pub async fn complete_mfa(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<MfaLoginInput>,
) -> Result<Response, ApiError> {
    validate_transport(&state, &headers, input.transport)?;
    let session = state
        .app
        .identity
        .complete_mfa_login(&input.challenge, input.proof.into(), trace)
        .await?;
    Ok(session_response(&state, session, input.transport))
}
#[utoipa::path(post, path="/api/v1/auth/password/reset", tag="Authentication", request_body=EmailInput, responses((status=202,body=AcceptedOutput)))]
pub async fn request_reset(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<EmailInput>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .app
        .identity
        .request_password_reset(&input.email, trace)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AcceptedOutput { accepted: true }),
    ))
}
#[utoipa::path(post, path="/api/v1/auth/password/reset/confirm", tag="Authentication", request_body=ResetPasswordInput, responses((status=204)))]
pub async fn reset_password(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    ApiJson(input): ApiJson<ResetPasswordInput>,
) -> Result<Response, ApiError> {
    state
        .app
        .identity
        .reset_password(&input.token, input.new_password, trace)
        .await?;
    Ok(cleared_session_response(&state))
}
#[utoipa::path(get, path="/api/v1/account", tag="Account", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=200,body=AccountOutput)))]
pub async fn get_account(
    State(state): State<HttpState>,
    Session(actor): Session,
) -> Result<Json<AccountOutput>, ApiError> {
    Ok(Json(state.app.identity.get_account(&actor).await?.into()))
}
#[utoipa::path(delete, path="/api/v1/account", tag="Account", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=204)))]
pub async fn deactivate_account(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
) -> Result<Response, ApiError> {
    state.app.identity.deactivate_account(&actor, trace).await?;
    Ok(cleared_session_response(&state))
}
#[utoipa::path(put, path="/api/v1/account/password", tag="Account", security(("cookieAuth"=[]),("bearerAuth"=[])), request_body=ChangePasswordInput, responses((status=204)))]
pub async fn change_password(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiJson(input): ApiJson<ChangePasswordInput>,
) -> Result<Response, ApiError> {
    state
        .app
        .identity
        .change_password(&actor, input.current_password, input.new_password, trace)
        .await?;
    Ok(cleared_session_response(&state))
}
#[utoipa::path(get, path="/api/v1/sessions", tag="Sessions", security(("cookieAuth"=[]),("bearerAuth"=[])), params(PaginationQuery), responses((status=200,body=SessionPageOutput)))]
pub async fn list_sessions(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiQuery(query): ApiQuery<PaginationQuery>,
) -> Result<Json<SessionPageOutput>, ApiError> {
    let page = state
        .app
        .identity
        .list_sessions(&actor, query.limit, query.cursor.as_deref())
        .await?;
    Ok(Json(SessionPageOutput {
        items: page
            .items
            .into_iter()
            .map(|session| SessionOutput {
                id: session.id,
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                expires_at: session.expires_at,
            })
            .collect(),
        next_cursor: page.next_cursor,
    }))
}
#[utoipa::path(delete, path="/api/v1/sessions/{session_id}", tag="Sessions", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("session_id"=Uuid,Path)), responses((status=204)))]
pub async fn revoke_session(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiPath(session_id): ApiPath<Uuid>,
) -> Result<Response, ApiError> {
    state
        .app
        .identity
        .revoke_session(&actor, session_id, trace)
        .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    if session_id == actor.session_id() {
        cookies::clear_session(&mut response, &state.settings);
    }
    Ok(response)
}
#[utoipa::path(delete, path="/api/v1/sessions", tag="Sessions", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=204)))]
pub async fn revoke_all_sessions(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
) -> Result<Response, ApiError> {
    state
        .app
        .identity
        .revoke_all_sessions(&actor, trace)
        .await?;
    Ok(cleared_session_response(&state))
}
#[utoipa::path(post, path="/api/v1/sessions/current/rotate", tag="Sessions", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=200,body=LoginOutput)))]
pub async fn rotate_session(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let transport = if headers.contains_key(axum::http::header::AUTHORIZATION) {
        SessionTransport::Bearer
    } else {
        SessionTransport::Cookie
    };
    let session = state.app.identity.rotate_session(&actor, trace).await?;
    Ok(session_response(&state, session, transport))
}
#[utoipa::path(post, path="/api/v1/account/totp", tag="MFA", security(("cookieAuth"=[]),("bearerAuth"=[])), responses((status=200,body=TotpEnrollmentOutput)))]
pub async fn begin_totp(
    State(state): State<HttpState>,
    Session(actor): Session,
) -> Result<Json<TotpEnrollmentOutput>, ApiError> {
    let enrollment = state.app.identity.begin_totp_enrollment(&actor).await?;
    Ok(Json(TotpEnrollmentOutput {
        challenge: enrollment.challenge,
        authenticator_url: enrollment.authenticator_url,
    }))
}
#[utoipa::path(post, path="/api/v1/account/totp/confirm", tag="MFA", security(("cookieAuth"=[]),("bearerAuth"=[])), request_body=TotpConfirmationInput, responses((status=200,body=RecoveryCodesOutput)))]
pub async fn confirm_totp(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiJson(input): ApiJson<TotpConfirmationInput>,
) -> Result<Response, ApiError> {
    let recovery_codes = state
        .app
        .identity
        .confirm_totp_enrollment(&actor, &input.challenge, &input.code, trace)
        .await?;
    let mut response = Json(RecoveryCodesOutput { recovery_codes }).into_response();
    cookies::clear_session(&mut response, &state.settings);
    Ok(response)
}
#[utoipa::path(delete, path="/api/v1/account/totp", tag="MFA", security(("cookieAuth"=[]),("bearerAuth"=[])), request_body=MfaProofInput, responses((status=204)))]
pub async fn disable_totp(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiJson(proof): ApiJson<MfaProofInput>,
) -> Result<Response, ApiError> {
    state
        .app
        .identity
        .disable_totp(&actor, proof.into(), trace)
        .await?;
    Ok(cleared_session_response(&state))
}

#[utoipa::path(post, path="/api/v1/account/recovery-codes", tag="MFA", security(("cookieAuth"=[]),("bearerAuth"=[])), request_body=MfaProofInput, responses((status=200,body=RecoveryCodesOutput)))]
pub async fn regenerate_recovery_codes(
    State(state): State<HttpState>,
    Extension(trace): Extension<TraceContext>,
    Session(actor): Session,
    ApiJson(proof): ApiJson<MfaProofInput>,
) -> Result<Response, ApiError> {
    let recovery_codes = state
        .app
        .identity
        .regenerate_recovery_codes(&actor, proof.into(), trace)
        .await?;
    let mut response = Json(RecoveryCodesOutput { recovery_codes }).into_response();
    cookies::clear_session(&mut response, &state.settings);
    Ok(response)
}

pub(super) fn login_response(
    state: &HttpState,
    outcome: LoginOutcome,
    transport: SessionTransport,
) -> Response {
    match outcome {
        LoginOutcome::Session(session) => session_response(state, session, transport),
        LoginOutcome::MfaRequired { challenge } => {
            Json(LoginOutput::MfaRequired { challenge }).into_response()
        }
    }
}
fn session_response(
    state: &HttpState,
    session: SessionCredentials,
    transport: SessionTransport,
) -> Response {
    let token = matches!(transport, SessionTransport::Bearer).then(|| session.token.clone());
    let mut response = Json(LoginOutput::Authenticated {
        session_id: session.session_id,
        expires_at: session.expires_at,
        token,
    })
    .into_response();
    if matches!(transport, SessionTransport::Cookie) {
        cookies::set_cookie(
            &mut response,
            &state.settings,
            state.settings.session_cookie_name(),
            session.token,
            (session.expires_at - chrono::Utc::now()).num_seconds(),
        );
    }
    response
}
fn cleared_session_response(state: &HttpState) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    cookies::clear_session(&mut response, &state.settings);
    response
}
fn validate_transport(
    state: &HttpState,
    headers: &HeaderMap,
    transport: SessionTransport,
) -> Result<(), ApiError> {
    if matches!(transport, SessionTransport::Cookie) {
        require_browser_origin(state, headers)?;
    }
    Ok(())
}
