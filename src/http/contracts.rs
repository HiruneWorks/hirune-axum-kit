use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailInput {
    pub email: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterInput {
    pub email: String,
    pub password: String,
}
#[derive(Default, Deserialize, ToSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SessionTransport {
    #[default]
    Cookie,
    Bearer,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub transport: SessionTransport,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenInput {
    pub token: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetPasswordInput {
    pub token: String,
    pub new_password: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(
    tag = "kind",
    content = "code",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MfaProofInput {
    Totp(String),
    RecoveryCode(String),
}
impl From<MfaProofInput> for application::identity::MfaProof {
    fn from(proof: MfaProofInput) -> Self {
        match proof {
            MfaProofInput::Totp(code) => Self::Totp(code),
            MfaProofInput::RecoveryCode(code) => Self::RecoveryCode(code),
        }
    }
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MfaLoginInput {
    pub challenge: String,
    pub proof: MfaProofInput,
    #[serde(default)]
    pub transport: SessionTransport,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TotpConfirmationInput {
    pub challenge: String,
    pub code: String,
}
#[derive(Serialize, ToSchema)]
pub struct TotpEnrollmentOutput {
    pub challenge: String,
    pub authenticator_url: String,
}
#[derive(Serialize, ToSchema)]
pub struct RecoveryCodesOutput {
    pub recovery_codes: Vec<String>,
}
#[derive(Serialize, ToSchema)]
pub struct AcceptedOutput {
    pub accepted: bool,
}
#[derive(Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginOutput {
    Authenticated {
        session_id: Uuid,
        expires_at: DateTime<Utc>,
        #[serde(skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
    MfaRequired {
        challenge: String,
    },
}
#[derive(Serialize, ToSchema)]
pub struct AccountOutput {
    pub id: Uuid,
    pub email: String,
    pub verified: bool,
    pub mfa_enabled: bool,
}
impl From<application::identity::AccountSummary> for AccountOutput {
    fn from(account: application::identity::AccountSummary) -> Self {
        Self {
            id: account.id,
            email: account.email,
            verified: account.verified,
            mfa_enabled: account.mfa_enabled,
        }
    }
}
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PaginationQuery {
    pub limit: Option<u64>,
    pub cursor: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct SessionOutput {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
#[derive(Serialize, ToSchema)]
pub struct SessionPageOutput {
    pub items: Vec<SessionOutput>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInput {
    pub name: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameOrganizationInput {
    pub name: String,
    pub expected_version: i64,
}
#[derive(Serialize, ToSchema)]
pub struct OrganizationOutput {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub version: i64,
}
impl From<application::organizations::OrganizationSummary> for OrganizationOutput {
    fn from(organization: application::organizations::OrganizationSummary) -> Self {
        Self {
            id: organization.id,
            name: organization.name,
            owner_id: organization.owner_id,
            version: organization.version,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct OrganizationPageOutput {
    pub items: Vec<OrganizationOutput>,
    pub next_cursor: Option<String>,
}
#[derive(Deserialize, IntoParams)]
pub struct OidcCallbackQuery {
    pub state: String,
    pub code: Option<String>,
    pub error: Option<String>,
    pub iss: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct OidcRedirectOutput {
    pub authorization_url: String,
}
#[derive(Serialize, ToSchema)]
pub struct ExternalIdentityOutput {
    pub id: Uuid,
    pub issuer: String,
}
