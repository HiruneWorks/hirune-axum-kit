use super::validation::{api_access_name, api_client_scopes};
use application::api_access::{ApiClientSummary, ApiKeySummary, QuotaStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;
#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiClientInput {
    #[validate(custom(function = "api_access_name"))]
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
    /// Allowed values: organizations:read. Empty scopes deny all external operations.
    #[validate(custom(function = "api_client_scopes"))]
    pub scopes: Vec<String>,
}
#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueApiKeyInput {
    #[validate(custom(function = "api_access_name"))]
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
    #[validate(range(min = 1, max = 365))]
    #[schema(minimum = 1, maximum = 365)]
    pub expires_in_days: i64,
}
#[derive(Serialize, ToSchema)]
pub struct QuotaOutput {
    pub minute_limit: i64,
    pub minute_used: i64,
    pub minute_resets_at: DateTime<Utc>,
    pub month_limit: i64,
    pub month_used: i64,
    pub month_resets_at: DateTime<Utc>,
}
impl From<QuotaStatus> for QuotaOutput {
    fn from(value: QuotaStatus) -> Self {
        Self {
            minute_limit: value.minute_limit,
            minute_used: value.minute_used,
            minute_resets_at: value.minute_resets_at,
            month_limit: value.month_limit,
            month_used: value.month_used,
            month_resets_at: value.month_resets_at,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct ApiClientOutput {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub active: bool,
    pub quota: QuotaOutput,
}
impl From<ApiClientSummary> for ApiClientOutput {
    fn from(value: ApiClientSummary) -> Self {
        Self {
            id: value.id,
            organization_id: value.organization_id,
            name: value.name,
            scopes: value.scopes,
            active: value.active,
            quota: value.quota.into(),
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct ApiKeyOutput {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
impl From<ApiKeySummary> for ApiKeyOutput {
    fn from(value: ApiKeySummary) -> Self {
        Self {
            id: value.id,
            name: value.name,
            created_at: value.created_at,
            expires_at: value.expires_at,
            revoked_at: value.revoked_at,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct IssuedApiKeyOutput {
    pub key: ApiKeyOutput,
    /// Returned once. Store securely; the server retains only a hash.
    pub credential: String,
}
#[derive(Serialize, ToSchema)]
pub struct ApiClientPageOutput {
    pub items: Vec<ApiClientOutput>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ApiKeyPageOutput {
    pub items: Vec<ApiKeyOutput>,
    pub next_cursor: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct ExternalOrganizationOutput {
    pub id: Uuid,
    pub name: String,
    pub version: i64,
}
