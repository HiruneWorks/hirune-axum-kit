use super::*;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, ExprTrait, IntoActiveModel, QuerySelect, TransactionTrait,
    sea_query::{Alias, Expr, Func, Query},
};

async fn wait_for_file_lock(
    database: &DatabaseConnection,
    worker_backend_pid: i32,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let blocked = database
                .query_one(
                    &Query::select()
                        .expr(Expr::value(1))
                        .from(Alias::new("pg_stat_activity"))
                        .and_where(
                            Expr::col(Alias::new("datname")).eq(Func::cust("current_database")),
                        )
                        .and_where(Expr::col(Alias::new("state")).eq("active"))
                        .and_where(Expr::col(Alias::new("wait_event_type")).eq("Lock"))
                        .and_where(
                            Expr::col(Alias::new("query"))
                                .like("SELECT%FROM \"files\"%FOR UPDATE%"),
                        )
                        .and_where(
                            Expr::expr(
                                Func::cust("array_position")
                                    .arg(
                                        Func::cust("pg_blocking_pids")
                                            .arg(Expr::col(Alias::new("pid"))),
                                    )
                                    .arg(worker_backend_pid),
                            )
                            .is_not_null(),
                        )
                        .limit(1)
                        .to_owned(),
                )
                .await?;
            if blocked.is_some() {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .context("authorized deletion did not wait for the worker's file lock")??;
    Ok(())
}

async fn exercise_cleanup_lock_order(
    database: &TestDatabase,
    app: &Application,
) -> anyhow::Result<()> {
    let (actor, _) = database.verified_account(&app.identity).await?;
    let organization = app
        .organizations
        .create_organization(&actor, "Cleanup lock order".into())
        .await?;
    let reservation = app
        .files
        .reserve_file(
            &actor,
            organization.id,
            ReserveFile {
                filename: "lock-order.bin".into(),
                media_type: None,
                byte_size: 0,
            },
            TraceContext::default(),
        )
        .await?;
    app.files
        .delete_file(
            &actor,
            organization.id,
            reservation.id,
            TraceContext::default(),
        )
        .await?;

    let transaction = database.connection.begin().await?;
    let worker_backend_pid: i32 = transaction
        .query_one(
            &Query::select()
                .expr_as(Func::cust("pg_backend_pid"), "backend_pid")
                .to_owned(),
        )
        .await?
        .context("worker backend was unavailable")?
        .try_get("", "backend_pid")?;
    let current = file::Entity::find_by_id(reservation.id)
        .lock_exclusive()
        .one(&transaction)
        .await?
        .context("file reservation was missing")?;
    anyhow::ensure!(
        current.state == application::files::FileState::Deleting,
        "file was not awaiting cleanup"
    );
    let expected_version = current.version + 1;
    let mut deleted = current.into_active_model();
    deleted.state = Set(application::files::FileState::Deleted);
    deleted.version = Set(expected_version);
    deleted.updated_at = Set(chrono::Utc::now());
    deleted.search_reconciled_at = Set(None);
    deleted.update(&transaction).await?;

    let mut deletion = tokio::spawn({
        let files = app.files.clone();
        async move {
            files
                .delete_file(
                    &actor,
                    organization.id,
                    reservation.id,
                    TraceContext::default(),
                )
                .await
        }
    });
    let worker_result: anyhow::Result<()> = async {
        wait_for_file_lock(&database.connection, worker_backend_pid).await?;
        // A second child-row update can reacquire foreign-key locks on its parents.
        let updated = tokio::time::timeout(
            Duration::from_secs(10),
            file::Entity::update_many()
                .col_expr(
                    file::Column::CleanupCompletedAt,
                    Expr::value(Some(chrono::Utc::now())),
                )
                .filter(file::Column::Id.eq(reservation.id))
                .filter(file::Column::State.eq(application::files::FileState::Deleted))
                .exec(&transaction),
        )
        .await
        .context("worker cleanup completion was blocked by parent locks")??;
        anyhow::ensure!(
            updated.rows_affected == 1,
            "worker did not complete cleanup"
        );
        tokio::time::timeout(Duration::from_secs(10), transaction.commit())
            .await
            .context("worker cleanup transaction did not commit")??;
        Ok(())
    }
    .await;
    if let Err(error) = worker_result {
        deletion.abort();
        let _ = deletion.await;
        return Err(error);
    }
    match tokio::time::timeout(Duration::from_secs(10), &mut deletion).await {
        Ok(result) => result.context("authorized deletion task failed")??,
        Err(error) => {
            deletion.abort();
            let _ = deletion.await;
            return Err(error).context("authorized deletion remained blocked after cleanup");
        }
    }
    let current = file::Entity::find_by_id(reservation.id)
        .one(&database.connection)
        .await?
        .context("cleaned file was missing")?;
    anyhow::ensure!(
        current.state == application::files::FileState::Deleted,
        "cleaned file state changed"
    );
    anyhow::ensure!(
        current.cleanup_completed_at.is_some(),
        "cleanup completion was not recorded"
    );
    anyhow::ensure!(
        current.version == expected_version,
        "idempotent deletion changed the cleaned file version"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires PostgreSQL and file settings; run just test-files"]
async fn file_cleanup_updates_do_not_deadlock_with_authorized_deletion() -> anyhow::Result<()> {
    let database = TestDatabase::create().await?;
    let result: anyhow::Result<()> = async {
        let mut settings = support::settings(&database)?;
        settings.files = integration_settings()?;
        let app = bootstrap(&settings).await?;
        let result = exercise_cleanup_lock_order(&database, &app).await;
        let closed = app.close().await;
        result?;
        closed
    }
    .await;
    let cleanup = database.finish().await;
    result?;
    cleanup
}
