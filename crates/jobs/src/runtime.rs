use crate::{Claim, JobError, PostgresQueue};
use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};
use tokio::task::{Id, JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct WorkerSettings {
    pub concurrency: usize,
    pub lease_seconds: i32,
    pub execution_seconds: i32,
    pub drain_seconds: u64,
    /// Delay between lookups after no eligible work is found.
    pub poll_milliseconds: u64,
}

impl Default for WorkerSettings {
    fn default() -> Self {
        Self {
            concurrency: 4,
            lease_seconds: 30,
            execution_seconds: 120,
            drain_seconds: 35,
            poll_milliseconds: 250,
        }
    }
}

impl WorkerSettings {
    pub fn validate(&self) -> Result<(), JobError> {
        if self.concurrency == 0
            || self.concurrency > 64
            || self.lease_seconds < 3
            || self.execution_seconds < self.lease_seconds
            || self.execution_seconds > 3600
            || self.poll_milliseconds == 0
            || self.drain_seconds == 0
        {
            return Err(JobError::Invalid);
        }
        Ok(())
    }
}

pub struct HandlerFailure {
    pub code: &'static str,
    pub retryable: bool,
}

#[derive(Clone, Copy)]
struct JobTaskIdentity {
    job_id: Uuid,
    attempt_id: Uuid,
    request_id: Uuid,
}

impl From<&Claim> for JobTaskIdentity {
    fn from(claim: &Claim) -> Self {
        Self {
            job_id: claim.id,
            attempt_id: claim.claim_token,
            request_id: claim.request_id,
        }
    }
}

fn record_job_error(claim: &Claim, operation: &'static str, error: &JobError) {
    let category = match error {
        JobError::Storage(_) => "storage",
        JobError::LeaseLost => "lease_lost",
        JobError::Invalid => "invalid",
    };
    tracing::error!(
        job_id = %claim.id,
        attempt_id = %claim.claim_token,
        request_id = %claim.request_id,
        error_code = "job_storage_or_lease",
        error_category = category,
        operation,
        "job attempt failed; lease recovery remains authoritative"
    );
}

fn record_task_result(
    result: Result<(Id, Result<(), JobError>), JoinError>,
    identities: &mut HashMap<Id, JobTaskIdentity>,
) {
    let error = match result {
        Ok((task_id, _)) => {
            identities.remove(&task_id);
            return;
        }
        Err(error) => error,
    };
    let Some(identity) = identities.remove(&error.id()) else {
        tracing::error!(
            task_id = %error.id(),
            error_code = "job_task_identity_missing",
            "job task failed without claim identity; lease recovery remains authoritative"
        );
        return;
    };
    if error.is_panic() {
        tracing::error!(
            job_id = %identity.job_id,
            attempt_id = %identity.attempt_id,
            request_id = %identity.request_id,
            error_code = "handler_panic",
            error_category = "panic",
            "job task failed; lease recovery remains authoritative"
        );
    } else {
        tracing::info!(
            job_id = %identity.job_id,
            attempt_id = %identity.attempt_id,
            request_id = %identity.request_id,
            error_code = "job_task_cancelled",
            error_category = "cancelled",
            "job task cancelled; lease recovery remains authoritative"
        );
    }
}

pub async fn run_one<F: Future<Output = Result<(), HandlerFailure>>>(
    queue: &PostgresQueue,
    claim: Claim,
    settings: WorkerSettings,
    handler: F,
) -> Result<(), JobError> {
    let span = tracing::info_span!("job", job_id=%claim.id, attempt_id=%claim.claim_token,
        attempt=claim.attempts, job_kind=%claim.kind, request_id=%claim.request_id);
    async {
        let mut heartbeat =
            tokio::time::interval(Duration::from_secs((settings.lease_seconds / 3) as u64));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The monotonic execution deadline also limits a heartbeat that waits.
        let execution = async {
            tokio::pin!(handler);
            loop {
                tokio::select! {
                    result = &mut handler => break Ok::<_, JobError>(result),
                    _ = heartbeat.tick() => queue.heartbeat(&claim, settings.lease_seconds).await?,
                }
            }
        };
        let result = tokio::time::timeout_at(claim.execution_deadline, execution).await;
        let failure = match result {
            Ok(Ok(Ok(()))) => None,
            Ok(Ok(Err(failure))) => Some((failure.code, failure.retryable)),
            Ok(Err(error)) => {
                record_job_error(&claim, "heartbeat", &error);
                return Err(error);
            }
            Err(_) => Some(("execution_timeout", true)),
        };
        tracing::info!(
            outcome = failure.map_or("succeeded", |(code, _)| code),
            "job execution finished"
        );
        let result = queue.finish(&claim, failure).await;
        if let Err(error) = &result {
            record_job_error(&claim, "finalize", error);
        }
        result
    }
    .instrument(span)
    .await
}

pub async fn run<H, F>(
    queue: PostgresQueue,
    settings: WorkerSettings,
    shutdown: CancellationToken,
    handler: H,
) -> Result<(), JobError>
where
    H: Fn(Claim) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Result<(), HandlerFailure>> + Send + 'static,
{
    settings.validate()?;
    let handler = Arc::new(handler);
    let mut tasks = JoinSet::new();
    let mut task_identities = HashMap::new();
    let empty_queue_poll_delay = Duration::from_millis(settings.poll_milliseconds);
    let mut next_claim_at = tokio::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            result = tasks.join_next_with_id(), if !tasks.is_empty() => {
                if let Some(result) = result {
                    record_task_result(result, &mut task_identities);
                }
            },
            _ = tokio::time::sleep_until(next_claim_at), if tasks.len() < settings.concurrency => {
                let claim = tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
                    claim = queue.claim(settings.lease_seconds, settings.execution_seconds) => claim?,
                };
                if let Some(claim) = claim {
                    let identity = JobTaskIdentity::from(&claim);
                    let queue = queue.clone();
                    let handler = Arc::clone(&handler);
                    let task = tasks.spawn(async move {
                        let handler_claim = claim.clone();
                        let execution = async move { handler(handler_claim).await };
                        run_one(&queue, claim, settings, execution).await
                    });
                    task_identities.insert(task.id(), identity);
                    next_claim_at = tokio::time::Instant::now();
                } else {
                    next_claim_at = tokio::time::Instant::now() + empty_queue_poll_delay;
                }
            },
        }
    }
    let drain = async {
        while let Some(result) = tasks.join_next_with_id().await {
            record_task_result(result, &mut task_identities);
        }
    };
    if tokio::time::timeout(Duration::from_secs(settings.drain_seconds), drain)
        .await
        .is_err()
    {
        tasks.abort_all();
        while let Some(result) = tasks.join_next_with_id().await {
            record_task_result(result, &mut task_identities);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
