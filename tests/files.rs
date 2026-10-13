mod support;

use anyhow::Context;
use application::{
    ApplicationError,
    files::{FileSettings, ReserveFile, S3Settings, SearchSettings, entities::file},
    organizations::entities::membership,
};
use axum::http::{StatusCode, header};
use axum_test::TestServer;
use bytes::Bytes;
use futures_util::{StreamExt, stream};
use jobs::TraceContext;
use kit::{
    bootstrap::{Application, bootstrap},
    http::build_router,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use test_support::TestDatabase;
use uuid::Uuid;

fn integration_settings() -> anyhow::Result<FileSettings> {
    let endpoint: url::Url = std::env::var("TEST_S3_ENDPOINT")?.parse()?;
    let search: url::Url = std::env::var("TEST_MEILISEARCH_URL")?.parse()?;
    for url in [&endpoint, &search] {
        anyhow::ensure!(
            matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "file integration tests require loopback services"
        );
    }
    Ok(FileSettings {
        storage: Some(S3Settings {
            endpoint,
            bucket: "kit-files".into(),
            region: "us-east-1".into(),
            access_key: application::config::SecretString::new("kit-local-access"),
            secret_key: application::config::SecretString::new("kit-local-development-secret-only"),
            allow_http: true,
        }),
        search: Some(SearchSettings {
            endpoint: search,
            api_key: application::config::SecretString::new(
                "kit-local-meilisearch-key-development-only",
            ),
            index: format!("files_{}", Uuid::now_v7().simple()),
            allow_http: true,
        }),
        max_file_bytes: 1024 * 1024,
        organization_quota_bytes: 4 * 1024 * 1024,
        ..Default::default()
    })
}

fn body(bytes: impl Into<Bytes>) -> application::files::TransferStream {
    let bytes = bytes.into();
    stream::once(async move { Ok(bytes) }).boxed()
}

async fn drain_file_jobs(app: &Application) -> anyhow::Result<()> {
    while let Some(claim) = app.queue.claim(30, 120).await? {
        app.files
            .execute_job(&claim)
            .await
            .map_err(|error| anyhow::anyhow!("file worker failed: {}", error.code))?;
        app.queue.finish(&claim, None).await?;
    }
    Ok(())
}

async fn expire_file(database: &TestDatabase, file_id: Uuid) -> anyhow::Result<()> {
    file::Entity::update_many()
        .col_expr(
            file::Column::UploadExpiresAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::minutes(1)),
        )
        .filter(file::Column::Id.eq(file_id))
        .exec(&database.connection)
        .await?;
    Ok(())
}

fn s3_client(settings: &FileSettings) -> aws_sdk_s3::Client {
    let storage = settings.storage.as_ref().unwrap();
    aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .endpoint_url(storage.endpoint.as_str())
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                storage.access_key.expose(),
                storage.secret_key.expose(),
                None,
                None,
                "file-tests",
            ))
            .force_path_style(true)
            .build(),
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn original_bytes_ranges_and_filename_search_use_the_production_router() -> anyhow::Result<()>
{
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    let app = bootstrap(&settings).await?;
    app.files
        .initialize_integrations()
        .await
        .context("initial file integration provisioning")?;
    app.files
        .initialize_integrations()
        .await
        .context("repeated file integration provisioning")?;
    let (actor, credentials) = database.verified_account(&app.identity).await?;
    let (_, outsider) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Originals".into())
        .await?;
    let server = TestServer::new(build_router(app.clone(), settings.http));
    let authorization = format!("Bearer {}", credentials.token);
    let preflight = server
        .method(
            axum::http::Method::OPTIONS,
            "/api/v1/organizations/test/files/test/content",
        )
        .add_header(header::ORIGIN, "http://localhost:3000")
        .add_header(header::ACCESS_CONTROL_REQUEST_METHOD, "HEAD")
        .add_header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
        .await;
    assert!(
        preflight.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()?
            .contains("HEAD")
    );
    let collection = format!("/api/v1/organizations/{}/files", organization.id);
    let bytes = Bytes::from(
        [
            b"<script>serverMustNotExecute()</script>".as_slice(),
            &vec![0_u8; 40_000],
        ]
        .concat(),
    );
    let reserved = server.post(&collection).add_header(header::AUTHORIZATION, &authorization)
        .json(&json!({"filename": "proposal-original.html", "media_type": "text/html", "byte_size": bytes.len()})).await;
    reserved.assert_status(StatusCode::CREATED);
    let metadata: Value = reserved.json();
    let file_id = metadata["id"].as_str().unwrap();
    let item = format!("{collection}/{file_id}");
    let content = format!("{item}/content");
    server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .await
        .assert_status_not_found();
    server
        .put(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .bytes(bytes.clone())
        .await
        .assert_status_ok();
    server
        .put(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .bytes(bytes.clone())
        .await
        .assert_status_conflict();
    let downloaded = server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .await;
    downloaded.assert_status_ok();
    assert_eq!(downloaded.as_bytes(), &bytes);
    assert_eq!(
        downloaded.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    assert!(
        downloaded.headers()[header::CONTENT_DISPOSITION]
            .to_str()?
            .starts_with("attachment;")
    );
    assert_eq!(downloaded.headers()["x-content-type-options"], "nosniff");
    let head = server
        .method(axum::http::Method::HEAD, &content)
        .add_header(header::AUTHORIZATION, &authorization)
        .await;
    head.assert_status_ok();
    assert_eq!(
        head.headers()[header::CONTENT_LENGTH].to_str()?,
        bytes.len().to_string()
    );
    assert!(head.as_bytes().is_empty());
    let ranged = server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .add_header(header::RANGE, "bytes=0-7")
        .await;
    ranged.assert_status(StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.as_bytes().as_ref(), &bytes[..8]);
    let suffix = server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .add_header(header::RANGE, "bytes=-6")
        .await;
    suffix.assert_status(StatusCode::PARTIAL_CONTENT);
    assert_eq!(suffix.as_bytes().as_ref(), &bytes[bytes.len() - 6..]);
    let invalid_range = server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .add_header(header::RANGE, "bytes=999999-")
        .await;
    invalid_range.assert_status(StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        invalid_range.headers()[header::CONTENT_RANGE].to_str()?,
        format!("bytes */{}", bytes.len())
    );
    server
        .get(&content)
        .add_header(header::AUTHORIZATION, format!("Bearer {}", outsider.token))
        .await
        .assert_status_not_found();
    drain_file_jobs(&app).await?;
    let search: Value = server
        .get(&format!("{collection}/search?q=proposal"))
        .add_header(header::AUTHORIZATION, &authorization)
        .await
        .json();
    assert_eq!(search["items"][0]["id"], file_id);
    assert!(search.get("estimatedTotalHits").is_none());
    server
        .delete(&item)
        .add_header(header::AUTHORIZATION, &authorization)
        .await
        .assert_status_no_content();
    server
        .get(&content)
        .add_header(header::AUTHORIZATION, &authorization)
        .await
        .assert_status_not_found();
    let stale_search: Value = server
        .get(&format!("{collection}/search?q=proposal"))
        .add_header(header::AUTHORIZATION, &authorization)
        .await
        .json();
    assert_eq!(stale_search["items"], json!([]));
    drain_file_jobs(&app).await?;
    assert_eq!(
        file::Entity::find_by_id(Uuid::parse_str(file_id)?)
            .one(&database.connection)
            .await?
            .unwrap()
            .state,
        "deleted"
    );
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn mismatched_uploads_expire_and_empty_files_remain_valid_originals() -> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    let app = bootstrap(&settings).await?;
    app.files.initialize_integrations().await?;
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Lengths".into())
        .await?;
    for bytes in ["x", "too many bytes"] {
        let reservation = app
            .files
            .reserve_file(
                &actor,
                organization.id,
                ReserveFile {
                    filename: "unchanged.bin".into(),
                    media_type: None,
                    byte_size: 2,
                },
                TraceContext::default(),
            )
            .await?;
        assert!(
            app.files
                .upload_original(
                    &actor,
                    organization.id,
                    reservation.id,
                    body(bytes),
                    TraceContext::default()
                )
                .await
                .is_err()
        );
        assert!(matches!(
            app.files
                .download_original(&actor, organization.id, reservation.id, None)
                .await,
            Err(ApplicationError::NotFound)
        ));
        expire_file(&database, reservation.id).await?;
    }
    assert_eq!(app.files.reconcile_files().await?, 2);
    drain_file_jobs(&app).await?;
    let empty = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "empty.bin".into(),
                media_type: None,
                byte_size: 0,
            },
            TraceContext::default(),
        )
        .await?;
    app.files
        .upload_original(
            &actor,
            organization.id,
            empty.id,
            body(Bytes::new()),
            TraceContext::default(),
        )
        .await?;
    let content = app
        .files
        .download_original(&actor, organization.id, empty.id, None)
        .await?;
    assert_eq!(content.byte_length, 0);
    assert!(content.stream.collect::<Vec<_>>().await.is_empty());
    app.files
        .delete_file(&actor, organization.id, empty.id, TraceContext::default())
        .await?;
    drain_file_jobs(&app).await?;
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn concurrent_members_cannot_overreserve_organization_quota() -> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    settings.files.max_file_bytes = 3;
    settings.files.organization_quota_bytes = 3;
    settings.files.pending_file_limit = 2;
    let app = bootstrap(&settings).await?;
    let (owner, _) = database.verified_account(&app.identity).await?;
    let (member, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&owner, "Quota".into())
        .await?;
    membership::ActiveModel {
        id: Set(Uuid::now_v7()),
        organization_id: Set(organization.id),
        account_id: Set(member.account_id()),
        role: Set("member".into()),
    }
    .insert(&database.connection)
    .await?;
    let reserve = |actor| {
        app.files.reserve_file(
            actor,
            organization.id,
            ReserveFile {
                filename: "small.bin".into(),
                media_type: None,
                byte_size: 2,
            },
            TraceContext::default(),
        )
    };
    let (first, second) = tokio::join!(reserve(&owner), reserve(&member));
    assert!(matches!(
        (&first, &second),
        (Ok(_), Err(ApplicationError::FileQuotaExceeded))
            | (Err(ApplicationError::FileQuotaExceeded), Ok(_))
    ));
    let winner = if first.is_ok() { &owner } else { &member };
    app.files
        .reserve_file(
            winner,
            organization.id,
            ReserveFile {
                filename: "empty.bin".into(),
                media_type: None,
                byte_size: 0,
            },
            TraceContext::default(),
        )
        .await?;
    assert!(matches!(
        app.files
            .reserve_file(
                winner,
                organization.id,
                ReserveFile {
                    filename: "another-empty.bin".into(),
                    media_type: None,
                    byte_size: 0
                },
                TraceContext::default()
            )
            .await,
        Err(ApplicationError::FileQuotaExceeded)
    ));
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn deletion_defeats_late_upload_completion_and_reconciliation_removes_its_bytes()
-> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    let client = s3_client(&settings.files);
    let app = bootstrap(&settings).await?;
    app.files.initialize_integrations().await?;
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Delete race".into())
        .await?;
    let reservation = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "late.bin".into(),
                media_type: None,
                byte_size: 4,
            },
            TraceContext::default(),
        )
        .await?;
    let release = Arc::new(tokio::sync::Notify::new());
    let upload = tokio::spawn({
        let files = app.files.clone();
        let actor = actor.clone();
        let release = release.clone();
        async move {
            files
                .upload_original(
                    &actor,
                    organization.id,
                    reservation.id,
                    stream::once(async move {
                        release.notified().await;
                        Ok(Bytes::from_static(b"late"))
                    })
                    .boxed(),
                    TraceContext::default(),
                )
                .await
        }
    });
    for _ in 0..100 {
        if file::Entity::find_by_id(reservation.id)
            .one(&database.connection)
            .await?
            .unwrap()
            .state
            == "transferring"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    app.files
        .delete_file(
            &actor,
            organization.id,
            reservation.id,
            TraceContext::default(),
        )
        .await?;
    drain_file_jobs(&app).await?;
    release.notify_one();
    assert!(matches!(upload.await?, Err(ApplicationError::Conflict)));
    let key = format!("originals/{}/{}", organization.id, reservation.id);
    assert!(
        client
            .head_object()
            .bucket("kit-files")
            .key(&key)
            .send()
            .await
            .is_ok()
    );
    assert!(app.files.reconcile_files().await? >= 1);
    drain_file_jobs(&app).await?;
    assert!(
        client
            .head_object()
            .bucket("kit-files")
            .key(&key)
            .send()
            .await
            .is_err()
    );
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn cancellation_removes_temporary_bytes_and_downloads_hold_transfer_slots()
-> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let temporary = tempfile::tempdir()?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    settings.files.transfer_concurrency = 1;
    settings.files.temporary_directory = temporary.path().to_owned();
    let app = bootstrap(&settings).await?;
    app.files.initialize_integrations().await?;
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Transfer lifetime".into())
        .await?;
    let canceled = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "cancel.bin".into(),
                media_type: None,
                byte_size: 4,
            },
            TraceContext::default(),
        )
        .await?;
    let upload = tokio::spawn({
        let files = app.files.clone();
        let actor = actor.clone();
        async move {
            files
                .upload_original(
                    &actor,
                    organization.id,
                    canceled.id,
                    stream::pending().boxed(),
                    TraceContext::default(),
                )
                .await
        }
    });
    for _ in 0..100 {
        if std::fs::read_dir(temporary.path())?.next().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(std::fs::read_dir(temporary.path())?.next().is_some());
    upload.abort();
    assert!(matches!(upload.await, Err(error) if error.is_cancelled()));
    assert!(std::fs::read_dir(temporary.path())?.next().is_none());
    expire_file(&database, canceled.id).await?;
    app.files.reconcile_files().await?;
    drain_file_jobs(&app).await?;
    let ready = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "ready.bin".into(),
                media_type: None,
                byte_size: 4,
            },
            TraceContext::default(),
        )
        .await?;
    app.files
        .upload_original(
            &actor,
            organization.id,
            ready.id,
            body("data"),
            TraceContext::default(),
        )
        .await?;
    let first = app
        .files
        .download_original(&actor, organization.id, ready.id, None)
        .await?;
    assert!(matches!(
        app.files
            .download_original(&actor, organization.id, ready.id, None)
            .await,
        Err(ApplicationError::IntegrationUnavailable)
    ));
    drop(first);
    let second = app
        .files
        .download_original(&actor, organization.id, ready.id, None)
        .await?;
    drop(second);
    membership::Entity::delete_many()
        .filter(membership::Column::OrganizationId.eq(organization.id))
        .filter(membership::Column::AccountId.eq(actor.account_id()))
        .exec(&database.connection)
        .await?;
    assert!(matches!(
        app.files.get_file(&actor, organization.id, ready.id).await,
        Err(ApplicationError::NotFound)
    ));
    assert!(matches!(
        app.files
            .search_files(&actor, organization.id, "ready")
            .await,
        Err(ApplicationError::NotFound)
    ));
    membership::ActiveModel {
        id: Set(Uuid::now_v7()),
        organization_id: Set(organization.id),
        account_id: Set(actor.account_id()),
        role: Set("owner".into()),
    }
    .insert(&database.connection)
    .await?;
    app.files
        .delete_file(&actor, organization.id, ready.id, TraceContext::default())
        .await?;
    drain_file_jobs(&app).await?;
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn zero_byte_reservations_are_rate_limited_and_old_confirmed_tombstones_are_purged()
-> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    settings.files.reservations_per_minute = 1;
    let app = bootstrap(&settings).await?;
    app.files.initialize_integrations().await?;
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Bounded metadata".into())
        .await?;
    let input = || ReserveFile {
        filename: "empty.bin".into(),
        media_type: None,
        byte_size: 0,
    };
    let original = app
        .files
        .reserve_file(&actor, organization.id, input(), TraceContext::default())
        .await?;
    assert!(matches!(
        app.files
            .reserve_file(&actor, organization.id, input(), TraceContext::default())
            .await,
        Err(ApplicationError::RateLimited)
    ));
    app.files
        .upload_original(
            &actor,
            organization.id,
            original.id,
            body(Bytes::new()),
            TraceContext::default(),
        )
        .await?;
    drain_file_jobs(&app).await?;
    let mut disabled_settings = settings.files.clone();
    disabled_settings.search = None;
    let search_disabled = application::files::FileService::new(
        app.identity.clone(),
        app.organizations.clone(),
        disabled_settings,
    )?;
    search_disabled
        .delete_file(
            &actor,
            organization.id,
            original.id,
            TraceContext::default(),
        )
        .await?;
    while let Some(claim) = app.queue.claim(30, 120).await? {
        search_disabled
            .execute_job(&claim)
            .await
            .map_err(|error| anyhow::anyhow!("file deletion failed: {}", error.code))?;
        app.queue.finish(&claim, None).await?;
    }
    let old = chrono::Utc::now() - chrono::Duration::days(40);
    file::Entity::update_many()
        .col_expr(
            file::Column::CreatedAt,
            sea_orm::sea_query::Expr::value(old),
        )
        .col_expr(
            file::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(old),
        )
        .col_expr(
            file::Column::UploadExpiresAt,
            sea_orm::sea_query::Expr::value(old),
        )
        .filter(file::Column::Id.eq(original.id))
        .exec(&database.connection)
        .await?;
    search_disabled.reconcile_files().await?;
    assert!(
        file::Entity::find_by_id(original.id)
            .one(&database.connection)
            .await?
            .unwrap()
            .search_document_may_exist
    );
    app.files.repair_search().await?;
    drain_file_jobs(&app).await?;
    app.files.reconcile_files().await?;
    assert!(
        file::Entity::find_by_id(original.id)
            .one(&database.connection)
            .await?
            .is_none()
    );
    app.close().await?;
    database.finish().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL, SeaweedFS and Meilisearch; run just test-files"]
async fn cleanup_prioritizes_unscheduled_uploads_and_advances_between_hourly_batches()
-> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let mut settings = support::settings(&database)?;
    settings.files = integration_settings()?;
    settings.files.search = None;
    let app = bootstrap(&settings).await?;
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Cleanup fairness".into())
        .await?;
    let now = chrono::Utc::now();
    let rows: Vec<_> = (0..101)
        .map(|_| {
            let id = Uuid::now_v7();
            file::ActiveModel {
                id: Set(id),
                organization_id: Set(organization.id),
                uploader_id: Set(actor.account_id()),
                object_key: Set(format!("originals/{}/{}", organization.id, id)),
                filename: Set("old.bin".into()),
                byte_size: Set(0),
                state: Set("deleted".into()),
                upload_expires_at: Set(now - chrono::Duration::days(2)),
                updated_at: Set(now - chrono::Duration::days(1)),
                created_at: Set(now - chrono::Duration::days(1)),
                cleanup_scheduled_at: Set(Some(now - chrono::Duration::hours(2))),
                ..Default::default()
            }
        })
        .collect();
    file::Entity::insert_many(rows)
        .exec(&database.connection)
        .await?;
    let expired = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "expired.bin".into(),
                media_type: None,
                byte_size: 1,
            },
            TraceContext::default(),
        )
        .await?;
    expire_file(&database, expired.id).await?;
    assert_eq!(app.files.reconcile_files().await?, 100);
    assert_eq!(
        file::Entity::find_by_id(expired.id)
            .one(&database.connection)
            .await?
            .unwrap()
            .state,
        "deleting"
    );
    let old_schedule = now - chrono::Duration::minutes(90);
    use sea_orm::PaginatorTrait;
    assert_eq!(
        file::Entity::find()
            .filter(file::Column::CleanupScheduledAt.lte(old_schedule))
            .count(&database.connection)
            .await?,
        2
    );
    file::Entity::update_many()
        .col_expr(
            file::Column::CleanupScheduledAt,
            sea_orm::sea_query::Expr::value(Some(now - chrono::Duration::minutes(70))),
        )
        .filter(file::Column::CleanupScheduledAt.gt(now - chrono::Duration::minutes(5)))
        .exec(&database.connection)
        .await?;
    assert_eq!(app.files.reconcile_files().await?, 100);
    assert_eq!(
        file::Entity::find()
            .filter(file::Column::CleanupScheduledAt.lte(old_schedule))
            .count(&database.connection)
            .await?,
        0
    );
    app.close().await?;
    database.finish().await
}
