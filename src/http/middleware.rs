use super::{
    HttpState,
    error::{ApiError, ErrorDescription, ErrorOutput},
    extract::read_cookie,
};
use axum::{
    Json,
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jobs::TraceContext;
use std::time::{Duration, Instant};
use tracing::Instrument;
use uuid::Uuid;

pub async fn request_context(
    State(state): State<HttpState>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = Uuid::now_v7();
    request.extensions_mut().insert(TraceContext { request_id });
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    let span = tracing::info_span!("http_request", request_id=%request_id, method=%request.method(), route, status=tracing::field::Empty, duration_ms=tracing::field::Empty);
    async move {
        let started = Instant::now();
        let validation = validate_origin(&state, request.method(), request.headers());
        let mut response = match validation {
            Err(error) => error.into_response(),
            Ok(()) => match state.request_permits.clone().try_acquire_owned() {
                Err(_) => ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "busy",
                    "The service is busy.",
                )
                .into_response(),
                Ok(_permit) => match tokio::time::timeout(
                    Duration::from_secs(state.settings.timeout_seconds),
                    run_validated_request(&state, request, next),
                )
                .await
                {
                    Ok(response) => response,
                    Err(_) => ApiError::new(
                        StatusCode::REQUEST_TIMEOUT,
                        "request_timeout",
                        "The request timed out.",
                    )
                    .into_response(),
                },
            },
        };
        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            let description = response
                .extensions()
                .get::<ErrorDescription>()
                .cloned()
                .unwrap_or_else(|| ErrorDescription {
                    code: match status {
                        StatusCode::NOT_FOUND => "not_found",
                        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
                        StatusCode::PAYLOAD_TOO_LARGE => "body_too_large",
                        _ => "request_failed",
                    },
                    message: status.canonical_reason().unwrap_or("Request failed"),
                });
            *response.body_mut() = Json(ErrorOutput {
                error: description,
                request_id,
            })
            .into_response()
            .into_body();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response.headers_mut().remove(header::CONTENT_LENGTH);
        }
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id.to_string()).expect("UUID header"),
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response.headers_mut().insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        response
            .headers_mut()
            .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
        response.headers_mut().insert(
            "content-security-policy",
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        );
        if state.settings.secure_cookies {
            response.headers_mut().insert(
                "strict-transport-security",
                HeaderValue::from_static("max-age=31536000"),
            );
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        }
        tracing::Span::current()
            .record("status", status.as_u16())
            .record("duration_ms", started.elapsed().as_millis() as u64);
        tracing::info!("request completed");
        response
    }
    .instrument(span)
    .await
}

async fn run_validated_request(state: &HttpState, request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("", MatchedPath::as_str);
    if route.starts_with("/api/v1/auth/") && request.method() != Method::OPTIONS {
        let peer = request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|peer| peer.0.ip());
        let client =
            match client_address(peer, request.headers(), &state.settings.trusted_proxy_ips) {
                Ok(client) => client,
                Err(error) => return error.into_response(),
            };
        if let Err(error) = state
            .app
            .identity
            .limit_client_authentication(&client)
            .await
        {
            return ApiError::from(error).into_response();
        }
    }
    next.run(request).await
}

fn client_address(
    peer: Option<std::net::IpAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[std::net::IpAddr],
) -> Result<String, ApiError> {
    let Some(peer) = peer else {
        return Ok("in-process".into());
    };
    if !trusted_proxies.contains(&peer) {
        return Ok(peer.to_string());
    }
    let mut forwarded = headers.get_all("x-forwarded-for").iter();
    let address = forwarded
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<std::net::IpAddr>().ok());
    if forwarded.next().is_some() {
        return Err(ApiError::invalid_input());
    }
    address
        .map(|address| address.to_string())
        .ok_or_else(ApiError::invalid_input)
}

fn validate_origin(
    state: &HttpState,
    method: &Method,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }
    if headers.contains_key(header::ORIGIN) {
        require_browser_origin(state, headers)?;
    } else if read_cookie(headers, state.settings.session_cookie_name())?.is_some()
        || headers.contains_key("sec-fetch-site")
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "A trusted origin is required.",
        ));
    }
    Ok(())
}

pub fn require_browser_origin(state: &HttpState, headers: &HeaderMap) -> Result<(), ApiError> {
    let allowed = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| {
            state
                .settings
                .allowed_origins
                .iter()
                .any(|allowed| allowed == origin)
        });
    if !allowed {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "A trusted origin is required.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwarding_headers_require_an_explicit_proxy_and_one_address() {
        let proxy = "127.0.0.1".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("192.0.2.10"));
        assert_eq!(
            client_address(Some(proxy), &headers, &[]).unwrap(),
            "127.0.0.1"
        );
        assert_eq!(
            client_address(Some(proxy), &headers, &[proxy]).unwrap(),
            "192.0.2.10"
        );
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("192.0.2.10, 192.0.2.11"),
        );
        assert!(client_address(Some(proxy), &headers, &[proxy]).is_err());
    }
}
