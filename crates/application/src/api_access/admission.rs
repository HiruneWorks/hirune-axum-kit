use super::{
    ApiAccessService, ExternalOrganization,
    entities::api_usage,
    repository,
    types::{QuotaPeriods, quota_status},
    validation::ORGANIZATIONS_READ,
};
use crate::{ApplicationError, ApplicationResult, database::database_time};
use auth_core::{constant_time::constant_time_str_eq, token::hash_token};
use jobs::TraceContext;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set, TransactionTrait};
use uuid::Uuid;

fn key_id(credential: &str) -> ApplicationResult<Uuid> {
    let (id, secret) = credential
        .strip_prefix("hak_v1_")
        .and_then(|credential| credential.split_once('.'))
        .ok_or(ApplicationError::Unauthenticated)?;
    if id.len() != 32
        || secret.len() != 43
        || !secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ApplicationError::Unauthenticated);
    }
    Uuid::parse_str(id).map_err(|_| ApplicationError::Unauthenticated)
}
fn retry_seconds(until: chrono::DateTime<chrono::Utc>, now: chrono::DateTime<chrono::Utc>) -> u64 {
    ((until - now).num_milliseconds().max(1) as u64).div_ceil(1000)
}

impl ApiAccessService {
    #[tracing::instrument(skip_all, fields(request_id = %trace.request_id))]
    pub async fn read_organization(
        &self,
        credential: &str,
        trace: TraceContext,
    ) -> ApplicationResult<ExternalOrganization> {
        let key_id = key_id(credential)?;
        let snapshot = repository::find_key(&self.identity.database, key_id)
            .await?
            .ok_or(ApplicationError::Unauthenticated)?;
        let presented_hash = hash_token(&format!("external API key v1:{credential}"));
        // Reject unauthenticated traffic before it can contend with client authority.
        if snapshot.revoked_at.is_some()
            || !constant_time_str_eq(&snapshot.secret_hash, &presented_hash)
        {
            return Err(ApplicationError::Unauthenticated);
        }
        let transaction = self.identity.database.begin().await?;
        let client = repository::lock_client(&transaction, snapshot.client_id)
            .await?
            .filter(|client| client.active)
            .ok_or(ApplicationError::Unauthenticated)?;
        let key = repository::lock_key(&transaction, key_id)
            .await?
            .filter(|key| key.client_id == client.id)
            .ok_or(ApplicationError::Unauthenticated)?;
        // Read time after acquiring authority locks; waiting can cross expiry or quota boundaries.
        let now = database_time(&transaction).await?;
        if key.revoked_at.is_some()
            || key.expires_at <= now
            || !constant_time_str_eq(&key.secret_hash, &presented_hash)
        {
            return Err(ApplicationError::Unauthenticated);
        }
        let scopes: Vec<String> =
            serde_json::from_value(client.scopes.clone()).map_err(ApplicationError::internal)?;
        if !scopes.iter().any(|scope| scope == ORGANIZATIONS_READ) {
            return Err(ApplicationError::Forbidden);
        }
        if client.minute_limit == 0 || client.month_limit == 0 {
            return Err(ApplicationError::Forbidden);
        }
        let organization = self
            .organizations
            .read_for_api_client(&transaction, client.organization_id)
            .await?;
        let mut quota = quota_status(&client, now);
        if quota.month_used >= quota.month_limit {
            return Err(ApplicationError::ApiQuotaExceeded {
                retry_after_seconds: retry_seconds(quota.month_resets_at, now),
            });
        }
        if quota.minute_used >= quota.minute_limit {
            return Err(ApplicationError::ApiRateLimited {
                retry_after_seconds: retry_seconds(quota.minute_resets_at, now),
            });
        }
        let periods = QuotaPeriods::at(now);
        quota.minute_used += 1;
        quota.month_used += 1;
        let mut updated = client.into_active_model();
        updated.minute_started_at = Set(Some(periods.minute));
        updated.month_started_at = Set(Some(periods.month));
        updated.minute_used = Set(quota.minute_used);
        updated.month_used = Set(quota.month_used);
        updated.update(&transaction).await?;
        api_usage::ActiveModel {
            id: Set(Uuid::now_v7()),
            client_id: Set(key.client_id),
            key_id: Set(key.id),
            operation: Set(ORGANIZATIONS_READ.into()),
            request_id: Set(trace.request_id),
            admitted_at: Set(now),
        }
        .insert(&transaction)
        .await?;
        transaction.commit().await?;
        tracing::info!(client_id = %key.client_id, key_id = %key.id, outcome = "admitted", "external API request admitted");
        Ok(ExternalOrganization {
            id: organization.id,
            name: organization.name,
            version: organization.version,
            quota,
        })
    }

    pub async fn cleanup_usage(&self, retention_days: i64) -> ApplicationResult<u64> {
        if !(1..=36500).contains(&retention_days) {
            return Err(ApplicationError::InvalidInput(
                "invalid API usage retention",
            ));
        }
        let transaction = self.identity.database.begin().await?;
        let removed = repository::cleanup_usage(
            &transaction,
            retention_days,
            database_time(&transaction).await?,
        )
        .await?;
        transaction.commit().await?;
        Ok(removed)
    }
}
