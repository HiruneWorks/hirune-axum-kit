use super::validation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct EmailInput {
    #[validate(custom(function = "validation::email"))]
    pub email: String,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterInput {
    #[validate(custom(function = "validation::email"))]
    pub email: String,
    #[validate(custom(function = "validation::new_password"))]
    /// At least 15 Unicode characters and at most 1024 UTF-8 bytes.
    #[schema(min_length = 15)]
    pub password: String,
}
#[derive(Default, Deserialize, ToSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SessionTransport {
    #[default]
    Cookie,
    Bearer,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginInput {
    #[validate(custom(function = "validation::email"))]
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub transport: SessionTransport,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct TokenInput {
    #[validate(length(min = 1, max = 1024))]
    #[schema(min_length = 1, max_length = 1024)]
    pub token: String,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ResetPasswordInput {
    #[validate(length(min = 1, max = 1024))]
    #[schema(min_length = 1, max_length = 1024)]
    pub token: String,
    #[validate(custom(function = "validation::new_password"))]
    /// At least 15 Unicode characters and at most 1024 UTF-8 bytes.
    #[schema(min_length = 15)]
    pub new_password: String,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordInput {
    pub current_password: String,
    #[validate(custom(function = "validation::new_password"))]
    /// At least 15 Unicode characters and at most 1024 UTF-8 bytes.
    #[schema(min_length = 15)]
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
impl Validate for MfaProofInput {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let code = match self {
            Self::Totp(code) | Self::RecoveryCode(code) => code,
        };
        if code.is_empty() || code.len() > 1024 {
            let mut errors = ValidationErrors::new();
            errors.add("code", ValidationError::new("length"));
            return Err(errors);
        }
        Ok(())
    }
}
impl From<MfaProofInput> for application::identity::MfaProof {
    fn from(proof: MfaProofInput) -> Self {
        match proof {
            MfaProofInput::Totp(code) => Self::Totp(code),
            MfaProofInput::RecoveryCode(code) => Self::RecoveryCode(code),
        }
    }
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MfaLoginInput {
    #[validate(length(min = 1, max = 1024))]
    #[schema(min_length = 1, max_length = 1024)]
    pub challenge: String,
    #[validate(nested)]
    pub proof: MfaProofInput,
    #[serde(default)]
    pub transport: SessionTransport,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct TotpConfirmationInput {
    #[validate(length(min = 1, max = 1024))]
    #[schema(min_length = 1, max_length = 1024)]
    pub challenge: String,
    #[validate(length(min = 1, max = 1024))]
    #[schema(min_length = 1, max_length = 1024)]
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
#[derive(Deserialize, IntoParams, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct PaginationQuery {
    #[validate(range(min = 1, max = 100))]
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u64>,
    #[validate(length(max = 512))]
    #[param(max_length = 512)]
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
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInput {
    #[validate(custom(function = "validation::organization_name"))]
    /// Trimmed name must contain 1 to 100 Unicode characters.
    pub name: String,
}
#[derive(Deserialize, ToSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct RenameOrganizationInput {
    #[validate(custom(function = "validation::organization_name"))]
    /// Trimmed name must contain 1 to 100 Unicode characters.
    pub name: String,
    #[validate(range(min = 1))]
    #[schema(minimum = 1)]
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
#[derive(Deserialize, IntoParams, Validate)]
pub struct OidcCallbackQuery {
    #[validate(length(min = 1, max = 1024))]
    pub state: String,
    #[validate(length(min = 1, max = 8192))]
    pub code: Option<String>,
    #[validate(length(max = 256))]
    pub error: Option<String>,
    #[validate(length(max = 2048))]
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
