use anyhow::Result;
use axum::{Router, body::Body, http::StatusCode, routing::get};
use bytes::Bytes;
use futures_util::{StreamExt, stream};
use kit::runtime::{http_drain_timeout, serve};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Notify};
use tokio_util::sync::CancellationToken;

#[test]
fn http_drain_covers_supported_request_budgets_and_response_flush() {
    assert_eq!(
        http_drain_timeout(Duration::from_secs(15)),
        Duration::from_secs(30)
    );
    assert_eq!(
        http_drain_timeout(Duration::from_secs(60)),
        Duration::from_secs(65)
    );
}

#[tokio::test]
async fn shutdown_lets_an_active_request_finish_before_the_drain_deadline() -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let entered = Arc::new(Notify::new());
    let release = CancellationToken::new();
    let router = Router::new().route(
        "/",
        get({
            let entered = entered.clone();
            let release = release.clone();
            move || {
                let entered = entered.clone();
                let release = release.clone();
                async move {
                    entered.notify_one();
                    release.cancelled().await;
                    "completed"
                }
            }
        }),
    );
    let shutdown = CancellationToken::new();
    let mut server = tokio::spawn(serve(
        listener,
        router,
        shutdown.clone(),
        Duration::from_secs(1),
    ));
    let response = tokio::spawn(async move {
        reqwest::get(format!("http://{address}/"))
            .await?
            .text()
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    shutdown.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut server)
            .await
            .is_err()
    );
    release.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), response).await???,
        "completed"
    );
    tokio::time::timeout(Duration::from_secs(1), server).await???;
    Ok(())
}

#[tokio::test]
async fn shutdown_bounds_a_stream_that_never_finishes() -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let router = Router::new().route(
        "/",
        get(|| async {
            let chunks =
                stream::once(async { Ok::<_, Infallible>(Bytes::from_static(b"started")) })
                    .chain(stream::pending());
            (StatusCode::OK, Body::from_stream(chunks))
        }),
    );
    let shutdown = CancellationToken::new();
    let server = tokio::spawn(serve(
        listener,
        router,
        shutdown.clone(),
        Duration::from_millis(100),
    ));
    let mut response = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(format!("http://{address}/")),
    )
    .await??;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), response.chunk())
            .await??
            .as_deref(),
        Some(b"started".as_slice()),
    );
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), server).await???;
    Ok(())
}
