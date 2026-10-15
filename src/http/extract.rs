use super::{HttpState, error::ApiError};
use application::{ApplicationError, identity::AuthenticatedSession};
use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{HeaderMap, StatusCode, request::Parts},
};
use cookie::Cookie;
use serde::de::DeserializeOwned;
use validator::Validate;

pub struct ApiJson<T>(pub T);
impl<T: DeserializeOwned + Validate + Send, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(|error| {
                if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "body_too_large",
                        "The request body is too large.",
                    )
                } else {
                    ApiError::invalid_input()
                }
            })?;
        // Validation errors may contain submitted secrets; never log or serialize them.
        value.validate().map_err(|_| ApiError::invalid_input())?;
        Ok(Self(value))
    }
}
pub struct ApiQuery<T>(pub T);
impl<T: DeserializeOwned + Validate + Send, S: Send + Sync> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input())?;
        value.validate().map_err(|_| ApiError::invalid_input())?;
        Ok(Self(value))
    }
}
pub struct ApiPath<T>(pub T);
impl<T: DeserializeOwned + Send, S: Send + Sync> FromRequestParts<S> for ApiPath<T> {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| ApiError::invalid_input())
    }
}

pub struct Session(pub AuthenticatedSession);
impl FromRequestParts<HttpState> for Session {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &HttpState,
    ) -> Result<Self, Self::Rejection> {
        let token = session_token(&parts.headers, state.settings.session_cookie_name())?
            .ok_or(ApplicationError::Unauthenticated)?;
        Ok(Self(state.app.identity.authenticate_session(&token).await?))
    }
}

pub fn read_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    let mut result = None;
    for value in headers.get_all(axum::http::header::COOKIE) {
        let value = value.to_str().map_err(|_| ApiError::invalid_input())?;
        for cookie in Cookie::split_parse(value) {
            let cookie = cookie.map_err(|_| ApiError::invalid_input())?;
            if cookie.name() == name {
                if result.is_some() {
                    return Err(ApiError::invalid_input());
                }
                result = Some(cookie.value().to_owned());
            }
        }
    }
    Ok(result)
}

fn session_token(headers: &HeaderMap, cookie_name: &str) -> Result<Option<String>, ApiError> {
    let cookie = read_cookie(headers, cookie_name)?;
    let mut authorization = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    let bearer = authorization
        .next()
        .map(|header| {
            header
                .to_str()
                .ok()
                .and_then(|value| value.strip_prefix("Bearer "))
                .filter(|token| !token.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| ApiError::from(ApplicationError::Unauthenticated))
        })
        .transpose()?;
    if authorization.next().is_some() || (cookie.is_some() && bearer.is_some()) {
        return Err(ApiError::invalid_input());
    }
    Ok(bearer.or(cookie))
}
