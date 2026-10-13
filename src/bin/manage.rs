use kit::{bootstrap::bootstrap, config::Settings, observability};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::from_environment()?;
    let _telemetry = observability::initialize(&settings.observability)?;
    let app = bootstrap(&settings).await?;
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let result = match arguments.as_slice() {
        [command] if command == "cleanup" => {
            let authentication_records = app.identity.cleanup_expired_authentication().await?;
            let completed_jobs = app.queue.cleanup_succeeded().await?;
            tracing::info!(
                authentication_records,
                completed_jobs,
                "expired records removed"
            );
            Ok(())
        }
        [command, job_id] if command == "retry-job" => {
            let job_id = uuid::Uuid::parse_str(job_id)?;
            anyhow::ensure!(
                app.queue.retry_failed(job_id).await?,
                "job is not eligible for retry"
            );
            tracing::info!(%job_id, "failed job scheduled again");
            Ok(())
        }
        _ => Err(anyhow::anyhow!(
            "usage: manage cleanup | manage retry-job <uuid>"
        )),
    };
    app.close().await?;
    result
}
