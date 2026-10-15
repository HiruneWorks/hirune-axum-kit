use sea_orm_migration::prelude::*;
mod columns;
mod m20261013_000001_create_accounts;
mod m20261013_000002_create_sessions;
mod m20261013_000003_create_challenges;
mod m20261013_000004_create_recovery_codes;
mod m20261013_000005_create_external_identities;
mod m20261013_000006_create_oauth_flows;
mod m20261013_000007_create_rate_limits;
mod m20261013_000008_create_organizations;
mod m20261013_000009_create_memberships;
mod m20261013_000010_create_mail_deliveries;
mod m20261013_000011_create_jobs;
mod m20261013_000012_create_job_attempts;
mod m20261013_000013_create_audit_events;
mod m20261013_000014_create_files;
mod m20261015_000015_index_audit_retention;
mod m20261015_000016_index_job_retention;

pub struct Migrator;

pub async fn require_current_schema(database: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    if !Migrator::get_pending_migrations_read_only(database)
        .await?
        .is_empty()
    {
        return Err(DbErr::Custom(
            "database schema is not current; run migrations before starting the service".into(),
        ));
    }
    Ok(())
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261013_000001_create_accounts::Migration),
            Box::new(m20261013_000002_create_sessions::Migration),
            Box::new(m20261013_000003_create_challenges::Migration),
            Box::new(m20261013_000004_create_recovery_codes::Migration),
            Box::new(m20261013_000005_create_external_identities::Migration),
            Box::new(m20261013_000006_create_oauth_flows::Migration),
            Box::new(m20261013_000007_create_rate_limits::Migration),
            Box::new(m20261013_000008_create_organizations::Migration),
            Box::new(m20261013_000009_create_memberships::Migration),
            Box::new(m20261013_000010_create_mail_deliveries::Migration),
            Box::new(m20261013_000011_create_jobs::Migration),
            Box::new(m20261013_000012_create_job_attempts::Migration),
            Box::new(m20261013_000013_create_audit_events::Migration),
            Box::new(m20261013_000014_create_files::Migration),
            Box::new(m20261015_000015_index_audit_retention::Migration),
            Box::new(m20261015_000016_index_job_retention::Migration),
        ]
    }
}

pub async fn migrate(database: &sea_orm::DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm::{
        ConnectionTrait, TransactionTrait,
        sea_query::{Func, Query},
    };
    let transaction = database.begin().await?;
    let migration_lock = Query::select()
        .expr(Func::cust("pg_advisory_xact_lock").arg(7_423_186_901_i64))
        .to_owned();
    transaction.query_one(&migration_lock).await?;
    Migrator::up(&transaction, None).await?;
    transaction.commit().await
}
