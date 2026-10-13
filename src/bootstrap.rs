use crate::config::{DatabaseSettings, Settings};
use application::{
    authorization::Authorization,
    identity::{IdentityService, oidc::OidcService},
    organizations::OrganizationService,
};
use jobs::PostgresQueue;
use mail::Mailer;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub struct Application {
    pub identity: IdentityService,
    pub organizations: OrganizationService,
    pub oidc: OidcService,
    pub queue: PostgresQueue,
    pub mailer: Mailer,
    database: DatabaseConnection,
    accepting_requests: Arc<AtomicBool>,
}

pub async fn connect_database(settings: &DatabaseSettings) -> anyhow::Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(settings.url.expose());
    options
        .max_connections(settings.max_connections)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(5))
        .acquire_timeout(Duration::from_secs(5))
        .sqlx_logging(false)
        .sqlx_slow_statements_logging_settings(
            tracing::log::LevelFilter::Off,
            Duration::from_secs(1),
        );
    Ok(Database::connect(options).await?)
}

pub async fn bootstrap(settings: &Settings) -> anyhow::Result<Application> {
    let database = connect_database(&settings.database).await?;
    migration::require_current_schema(&database).await?;
    let authorization = Arc::new(Authorization::new()?);
    let identity =
        IdentityService::new(database.clone(), settings.identity.clone(), authorization).await?;
    let oidc = OidcService::discover(identity.clone(), settings.oidc.clone()).await?;
    let organizations = OrganizationService::new(identity.clone());
    let mailer = Mailer::new(&settings.smtp)?;
    Ok(Application {
        identity,
        organizations,
        oidc,
        queue: PostgresQueue::new(database.clone()),
        mailer,
        database,
        accepting_requests: Arc::new(AtomicBool::new(true)),
    })
}

impl Application {
    pub async fn is_ready(&self) -> bool {
        self.accepting_requests.load(Ordering::Acquire) && self.database.ping().await.is_ok()
    }
    pub fn begin_shutdown(&self) {
        self.accepting_requests.store(false, Ordering::Release);
    }
    pub async fn close(&self) -> anyhow::Result<()> {
        self.database.close_by_ref().await?;
        Ok(())
    }
}
