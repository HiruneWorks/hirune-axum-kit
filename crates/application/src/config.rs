use crate::{ApplicationError, ApplicationResult, identity::session_expiry::session_expires_at};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};
use url::Url;

#[derive(Clone)]
pub struct SecretString(String);
impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
pub struct SecretKeys {
    active_key_id: String,
    keys: Arc<BTreeMap<String, Vec<u8>>>,
}
#[derive(Serialize, Deserialize)]
struct EncryptedValue {
    key_id: String,
    ciphertext: String,
}
impl SecretKeys {
    pub fn new(
        active_key_id: String,
        encoded_keys: BTreeMap<String, String>,
    ) -> ApplicationResult<Self> {
        let keys: Result<BTreeMap<_, _>, _> = encoded_keys
            .into_iter()
            .map(|(key_id, encoded)| {
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| ApplicationError::InvalidInput("secret keys must use base64"))?;
                if key_id.is_empty() || key_id.len() > 40 || bytes.len() != 32 {
                    return Err(ApplicationError::InvalidInput(
                        "secret keys require an ID and 32 random bytes",
                    ));
                }
                Ok((key_id, bytes))
            })
            .collect();
        let keys = keys?;
        if !keys.contains_key(&active_key_id) {
            return Err(ApplicationError::InvalidInput(
                "active secret key is missing",
            ));
        }
        Ok(Self {
            active_key_id,
            keys: Arc::new(keys),
        })
    }
    pub fn encrypt(&self, context: &str, plaintext: &[u8]) -> ApplicationResult<String> {
        let ciphertext =
            auth_core::aead::encrypt(&self.keys[&self.active_key_id], context, plaintext).map_err(
                |_| ApplicationError::internal(anyhow::anyhow!("secret encryption failed")),
            )?;
        serde_json::to_string(&EncryptedValue {
            key_id: self.active_key_id.clone(),
            ciphertext,
        })
        .map_err(ApplicationError::internal)
    }
    pub fn decrypt(&self, context: &str, encoded: &str) -> ApplicationResult<Vec<u8>> {
        let encrypted: EncryptedValue =
            serde_json::from_str(encoded).map_err(ApplicationError::internal)?;
        let key = self.keys.get(&encrypted.key_id).ok_or_else(|| {
            ApplicationError::internal(anyhow::anyhow!("stored key ID unavailable"))
        })?;
        auth_core::aead::decrypt(key, context, &encrypted.ciphertext)
            .map_err(|_| ApplicationError::internal(anyhow::anyhow!("secret decryption failed")))
    }
}
impl fmt::Debug for SecretKeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretKeys")
            .field("active_key_id", &self.active_key_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct IdentitySettings {
    pub application_name: String,
    pub public_url: Url,
    pub secret_keys: SecretKeys,
    pub rate_limit_key: SecretString,
    pub password_concurrency: usize,
    pub authentication_identity_limit_per_minute: i32,
    pub authentication_client_limit_per_minute: i32,
    pub session_idle_seconds: i64,
    pub session_absolute_seconds: i64,
}
impl IdentitySettings {
    pub fn validate(&self) -> ApplicationResult<()> {
        if self.application_name.is_empty()
            || self.password_concurrency == 0
            || self.password_concurrency > 64
            || !(1..=MAX_AUTHENTICATION_ATTEMPTS_PER_MINUTE)
                .contains(&self.authentication_identity_limit_per_minute)
            || !(1..=MAX_AUTHENTICATION_ATTEMPTS_PER_MINUTE)
                .contains(&self.authentication_client_limit_per_minute)
            || self.session_idle_seconds < 60
            || self.session_absolute_seconds < self.session_idle_seconds
            || self.rate_limit_key.expose().len() < 32
        {
            return Err(ApplicationError::InvalidInput("invalid identity settings"));
        }
        let now = chrono::Utc::now();
        session_expires_at(now, self.session_idle_seconds)?;
        session_expires_at(now, self.session_absolute_seconds)?;
        Ok(())
    }
}

pub(crate) const MAX_AUTHENTICATION_ATTEMPTS_PER_MINUTE: i32 = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_settings() -> IdentitySettings {
        IdentitySettings {
            application_name: "Session settings test".into(),
            public_url: Url::parse("https://example.test/").unwrap(),
            secret_keys: SecretKeys::new(
                "test".into(),
                [("test".into(), STANDARD.encode([1_u8; 32]))].into(),
            )
            .unwrap(),
            rate_limit_key: SecretString::new("x".repeat(32)),
            password_concurrency: 4,
            authentication_identity_limit_per_minute: 10,
            authentication_client_limit_per_minute: 60,
            session_idle_seconds: 1_209_600,
            session_absolute_seconds: 7_776_000,
        }
    }

    #[test]
    fn session_lifetimes_accept_defaults_and_larger_representable_values() {
        let mut settings = identity_settings();
        assert!(settings.validate().is_ok());
        settings.session_idle_seconds = 100 * 365 * 24 * 60 * 60;
        settings.session_absolute_seconds = settings.session_idle_seconds;
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn session_lifetimes_reject_duration_and_date_overflow() {
        for (idle_seconds, absolute_seconds) in [
            (60, i64::MAX),
            (i64::MAX, i64::MAX),
            (60, chrono::Duration::MAX.num_seconds()),
            (
                chrono::Duration::MAX.num_seconds(),
                chrono::Duration::MAX.num_seconds(),
            ),
        ] {
            let mut settings = identity_settings();
            settings.session_idle_seconds = idle_seconds;
            settings.session_absolute_seconds = absolute_seconds;
            assert!(matches!(
                settings.validate(),
                Err(ApplicationError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn session_lifetimes_preserve_the_minimum_and_absolute_order() {
        for (idle_seconds, absolute_seconds) in [(59, 7_776_000), (120, 60)] {
            let mut settings = identity_settings();
            settings.session_idle_seconds = idle_seconds;
            settings.session_absolute_seconds = absolute_seconds;
            assert!(settings.validate().is_err());
        }
        let mut settings = identity_settings();
        settings.session_idle_seconds = 60;
        settings.session_absolute_seconds = 60;
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn rotation_keeps_old_ciphertext_readable_and_context_bound() {
        let old = STANDARD.encode([1_u8; 32]);
        let new = STANDARD.encode([2_u8; 32]);
        let original = SecretKeys::new("old".into(), [("old".into(), old.clone())].into()).unwrap();
        let ciphertext = original.encrypt("account:first", b"test secret").unwrap();
        let rotated = SecretKeys::new(
            "new".into(),
            [("old".into(), old), ("new".into(), new.clone())].into(),
        )
        .unwrap();
        assert_eq!(
            rotated.decrypt("account:first", &ciphertext).unwrap(),
            b"test secret"
        );
        assert!(rotated.decrypt("account:second", &ciphertext).is_err());
        let new_only = SecretKeys::new("new".into(), [("new".into(), new)].into()).unwrap();
        assert!(new_only.decrypt("account:first", &ciphertext).is_err());
        assert!(!format!("{rotated:?}").contains("test secret"));
    }
}
