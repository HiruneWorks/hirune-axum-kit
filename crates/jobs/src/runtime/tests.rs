use super::*;
use sea_orm::{DatabaseConnection, DbErr};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
        tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_writer(self.clone())
            .finish()
    }

    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }

    fn events(&self) -> Vec<serde_json::Value> {
        self.text()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Write for CapturedLogs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedLogs {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn claim() -> Claim {
    Claim {
        id: Uuid::now_v7(),
        kind: "test.execution".into(),
        payload: serde_json::json!({"private": "untrusted job payload"}),
        request_id: Uuid::now_v7(),
        attempts: 1,
        max_attempts: 5,
        claim_token: Uuid::now_v7(),
        deadline_at: chrono::Utc::now() + chrono::Duration::seconds(30),
        execution_deadline: tokio::time::Instant::now() + Duration::from_secs(30),
    }
}

fn assert_identity(event: &serde_json::Value, identity: JobTaskIdentity) {
    assert_eq!(event["fields"]["job_id"], identity.job_id.to_string());
    assert_eq!(
        event["fields"]["attempt_id"],
        identity.attempt_id.to_string()
    );
    assert_eq!(
        event["fields"]["request_id"],
        identity.request_id.to_string()
    );
}

#[tokio::test]
async fn heartbeat_storage_failure_retains_claim_context() {
    let claim = claim();
    let identity = JobTaskIdentity::from(&claim);
    let logs = CapturedLogs::default();
    let queue = PostgresQueue::new(DatabaseConnection::default());

    let result = run_one(
        &queue,
        claim,
        WorkerSettings::default(),
        std::future::pending(),
    )
    .with_subscriber(logs.subscriber())
    .await;

    assert!(matches!(result, Err(JobError::Storage(_))));
    let events = logs.events();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_identity(event, identity);
    assert_eq!(event["fields"]["error_code"], "job_storage_or_lease");
    assert_eq!(event["fields"]["error_category"], "storage");
    assert_eq!(event["fields"]["operation"], "heartbeat");
    assert!(!logs.text().contains("untrusted job payload"));
}

#[test]
fn job_failure_categories_exclude_storage_details() {
    let claim = claim();
    let logs = CapturedLogs::default();
    tracing::subscriber::with_default(logs.subscriber(), || {
        for error in [
            JobError::Storage(DbErr::Custom("untrusted database detail".into())),
            JobError::LeaseLost,
            JobError::Invalid,
        ] {
            record_job_error(&claim, "finalize", &error);
        }
    });

    let events = logs.events();
    let categories: Vec<_> = events
        .iter()
        .map(|event| event["fields"]["error_category"].as_str().unwrap())
        .collect();
    assert_eq!(categories, ["storage", "lease_lost", "invalid"]);
    assert!(events.iter().all(|event| {
        event["fields"]["error_code"] == "job_storage_or_lease"
            && event["fields"]["operation"] == "finalize"
    }));
    assert!(!logs.text().contains("untrusted database detail"));
}

#[tokio::test]
async fn panicked_task_retains_claim_identity_without_panic_details() {
    let identity = JobTaskIdentity::from(&claim());
    let mut tasks: JoinSet<Result<(), JobError>> = JoinSet::new();
    let task = tasks.spawn(async { panic!("untrusted panic detail") });
    let mut identities = HashMap::from([(task.id(), identity)]);
    let result = tasks.join_next_with_id().await.unwrap();
    let logs = CapturedLogs::default();

    tracing::subscriber::with_default(logs.subscriber(), || {
        record_task_result(result, &mut identities);
    });

    assert!(identities.is_empty());
    let events = logs.events();
    assert_eq!(events.len(), 1);
    assert_identity(&events[0], identity);
    assert_eq!(events[0]["fields"]["error_code"], "handler_panic");
    assert_eq!(events[0]["fields"]["error_category"], "panic");
    assert!(!logs.text().contains("untrusted panic detail"));
}

#[tokio::test]
async fn cancelled_task_retains_identity_in_an_informational_event() {
    let identity = JobTaskIdentity::from(&claim());
    let mut tasks: JoinSet<Result<(), JobError>> = JoinSet::new();
    let task = tasks.spawn(std::future::pending());
    let mut identities = HashMap::from([(task.id(), identity)]);
    task.abort();
    let result = tasks.join_next_with_id().await.unwrap();
    let logs = CapturedLogs::default();

    tracing::subscriber::with_default(logs.subscriber(), || {
        record_task_result(result, &mut identities);
    });

    assert!(identities.is_empty());
    let events = logs.events();
    assert_eq!(events.len(), 1);
    assert_identity(&events[0], identity);
    assert_eq!(events[0]["level"], "INFO");
    assert_eq!(events[0]["fields"]["error_category"], "cancelled");
}

#[tokio::test]
async fn completed_tasks_release_identity_without_duplicate_diagnostics() {
    let mut tasks = JoinSet::new();
    let mut identities = HashMap::new();
    for result in [Ok(()), Err(JobError::LeaseLost)] {
        let task = tasks.spawn(async move { result });
        identities.insert(task.id(), JobTaskIdentity::from(&claim()));
    }
    let logs = CapturedLogs::default();

    while let Some(result) = tasks.join_next_with_id().await {
        tracing::subscriber::with_default(logs.subscriber(), || {
            record_task_result(result, &mut identities);
        });
    }

    assert!(identities.is_empty());
    assert!(logs.text().is_empty());
}
