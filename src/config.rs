use anyhow::{Context, Result, ensure};
use application::{
    config::{IdentitySettings, SecretKeys, SecretString},
    identity::oidc::ProviderSettings,
};
use jobs::WorkerSettings;
use mail::SmtpSettings;
use std::{collections::BTreeMap, net::SocketAddr, str::FromStr};
use url::Url;

#[derive(Clone, Debug)]
pub struct DatabaseSettings {
    pub url: SecretString,
    pub max_connections: u32,
}

#[derive(Clone, Debug)]
pub struct HttpSettings {
    pub bind_address: SocketAddr,
    pub allowed_origins: Vec<String>,
    pub trusted_proxy_ips: Vec<std::net::IpAddr>,
    pub secure_cookies: bool,
    pub timeout_seconds: u64,
    pub max_concurrent_requests: usize,
}
impl HttpSettings {
    pub fn session_cookie_name(&self) -> &'static str {
        if self.secure_cookies {
            "__Host-session"
        } else {
            "session"
        }
    }
    pub fn oauth_cookie_name(&self) -> &'static str {
        if self.secure_cookies {
            "__Host-oidc"
        } else {
            "oidc"
        }
    }
}

#[derive(Clone, Debug)]
pub struct ObservabilitySettings {
    pub log_filter: String,
    pub sentry_dsn: Option<SecretString>,
    pub sentry_sample_rate: f32,
}

#[derive(Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub identity: IdentitySettings,
    pub http: HttpSettings,
    pub smtp: SmtpSettings,
    pub worker: WorkerSettings,
    pub oidc: Vec<ProviderSettings>,
    pub observability: ObservabilitySettings,
}

impl Settings {
    pub fn from_environment() -> Result<Self> {
        Self::from_values(&std::env::vars().collect())
    }

    pub fn from_values(values: &BTreeMap<String, String>) -> Result<Self> {
        let get = |name: &str, fallback: &str| {
            values.get(name).cloned().unwrap_or_else(|| fallback.into())
        };
        let required = |name: &str| {
            values
                .get(name)
                .filter(|value| !value.is_empty())
                .cloned()
                .with_context(|| format!("{name} is required"))
        };
        let production = get("APP_ENV", "development") == "production";
        ensure!(
            ["development", "production", "test"].contains(&get("APP_ENV", "development").as_str()),
            "invalid APP_ENV"
        );
        let public_url = Url::parse(&get("PUBLIC_URL", "http://localhost:3000/"))
            .context("invalid PUBLIC_URL")?;
        validate_web_url(&public_url, production)?;
        let allowed_origins: Vec<String> = get(
            "ALLOWED_ORIGINS",
            &public_url.origin().ascii_serialization(),
        )
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect();
        for origin in &allowed_origins {
            let url = Url::parse(origin).context("invalid ALLOWED_ORIGINS")?;
            validate_web_url(&url, production)?;
            ensure!(
                url.origin().ascii_serialization() == *origin,
                "ALLOWED_ORIGINS must contain exact origins without paths"
            );
        }
        let encoded_keys = serde_json::from_str(&required("SECRET_KEYS")?)
            .map_err(|_| anyhow::anyhow!("invalid SECRET_KEYS JSON map"))?;
        let identity = IdentitySettings {
            application_name: get("APP_NAME", "Hirune Axum Kit"),
            public_url,
            secret_keys: SecretKeys::new(required("ACTIVE_SECRET_KEY")?, encoded_keys)?,
            rate_limit_key: SecretString::new(required("RATE_LIMIT_KEY")?),
            password_concurrency: number(values, "PASSWORD_CONCURRENCY", 4)?,
            session_idle_seconds: number(values, "SESSION_IDLE_SECONDS", 1800)?,
            session_absolute_seconds: number(values, "SESSION_ABSOLUTE_SECONDS", 43_200)?,
        };
        identity.validate()?;
        let database_url = required("DATABASE_URL")?;
        let parsed_database =
            Url::parse(&database_url).map_err(|_| anyhow::anyhow!("invalid DATABASE_URL"))?;
        ensure!(
            ["postgres", "postgresql"].contains(&parsed_database.scheme()),
            "DATABASE_URL must use PostgreSQL"
        );
        let database = DatabaseSettings {
            url: SecretString::new(database_url),
            max_connections: number(values, "DATABASE_MAX_CONNECTIONS", 10)?,
        };
        ensure!(
            (1..=100).contains(&database.max_connections),
            "DATABASE_MAX_CONNECTIONS must be between 1 and 100"
        );
        let http = HttpSettings {
            bind_address: number(
                values,
                "BIND_ADDRESS",
                SocketAddr::from(([127, 0, 0, 1], 3000)),
            )?,
            allowed_origins,
            trusted_proxy_ips: get("TRUSTED_PROXY_IPS", "")
                .split(',')
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.trim().parse().context("invalid TRUSTED_PROXY_IPS"))
                .collect::<Result<_>>()?,
            secure_cookies: production,
            timeout_seconds: number(values, "HTTP_TIMEOUT_SECONDS", 15)?,
            max_concurrent_requests: number(values, "HTTP_MAX_CONCURRENT_REQUESTS", 128)?,
        };
        ensure!(
            (1..=60).contains(&http.timeout_seconds)
                && (1..=4096).contains(&http.max_concurrent_requests),
            "invalid HTTP limits"
        );
        let smtp = SmtpSettings {
            host: get("SMTP_HOST", "localhost"),
            port: number(values, "SMTP_PORT", 1025)?,
            starttls: number(values, "SMTP_STARTTLS", false)?,
            username: values
                .get("SMTP_USERNAME")
                .filter(|value| !value.is_empty())
                .cloned(),
            password: values
                .get("SMTP_PASSWORD")
                .filter(|value| !value.is_empty())
                .cloned(),
            sender: get("MAIL_FROM", "Hirune <noreply@example.test>"),
        };
        ensure!(
            !production || smtp.starttls,
            "production SMTP requires STARTTLS"
        );
        let worker = WorkerSettings {
            concurrency: number(values, "WORKER_CONCURRENCY", 4)?,
            lease_seconds: number(values, "JOB_LEASE_SECONDS", 30)?,
            execution_seconds: number(values, "JOB_EXECUTION_SECONDS", 120)?,
            drain_seconds: number(values, "WORKER_DRAIN_SECONDS", 35)?,
            poll_milliseconds: number(values, "WORKER_POLL_MILLISECONDS", 250)?,
        };
        worker.validate()?;
        let mut oidc = Vec::new();
        if let Some(issuer) = values.get("OIDC_ISSUER").filter(|value| !value.is_empty()) {
            let issuer = Url::parse(issuer).context("invalid OIDC_ISSUER")?;
            let redirect_url =
                Url::parse(&required("OIDC_REDIRECT_URL")?).context("invalid OIDC_REDIRECT_URL")?;
            validate_web_url(&issuer, production)?;
            validate_web_url(&redirect_url, production)?;
            oidc.push(ProviderSettings {
                name: get("OIDC_PROVIDER", "default"),
                issuer: required("OIDC_ISSUER")?,
                client_id: required("OIDC_CLIENT_ID")?,
                client_secret: SecretString::new(required("OIDC_CLIENT_SECRET")?),
                redirect_url,
            });
        }
        let observability = ObservabilitySettings {
            log_filter: get(
                "RUST_LOG",
                "info,sqlx=off,sea_orm=off,reqwest=off,hyper=off",
            ),
            sentry_dsn: values
                .get("SENTRY_DSN")
                .filter(|value| !value.is_empty())
                .cloned()
                .map(SecretString::new),
            sentry_sample_rate: number(values, "SENTRY_SAMPLE_RATE", 0.05)?,
        };
        ensure!(
            (0.0..=1.0).contains(&observability.sentry_sample_rate),
            "invalid SENTRY_SAMPLE_RATE"
        );
        Ok(Self {
            database,
            identity,
            http,
            smtp,
            worker,
            oidc,
            observability,
        })
    }
}

fn number<T: FromStr>(values: &BTreeMap<String, String>, name: &str, fallback: T) -> Result<T> {
    match values.get(name) {
        Some(value) => value.parse().map_err(|_| anyhow::anyhow!("invalid {name}")),
        None => Ok(fallback),
    }
}

fn validate_web_url(url: &Url, production: bool) -> Result<()> {
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "web URLs cannot contain credentials, queries or fragments"
    );
    ensure!(
        url.scheme() == "https"
            || (!production
                && url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
        "HTTPS is required outside local development"
    );
    Ok(())
}
