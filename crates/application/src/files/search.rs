use super::entities::file;
use crate::{ApplicationError, ApplicationResult, config::SecretString};
use futures_util::StreamExt;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SearchSettings {
    pub endpoint: Url,
    pub api_key: SecretString,
    pub index: String,
    pub allow_http: bool,
}

impl SearchSettings {
    pub fn validate(&self) -> ApplicationResult<()> {
        if !(self.endpoint.scheme() == "https"
            || (self.allow_http && self.endpoint.scheme() == "http"))
            || self.endpoint.host_str().is_none()
            || !self.endpoint.username().is_empty()
            || self.endpoint.password().is_some()
            || self.endpoint.query().is_some()
            || self.endpoint.fragment().is_some()
            || self.api_key.expose().is_empty()
            || self.index.is_empty()
            || self.index.len() > 64
            || !self
                .index
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(ApplicationError::InvalidInput(
                "invalid Meilisearch settings",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct FileSearch {
    client: Client,
    settings: SearchSettings,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmittedTask {
    task_uid: u64,
}
#[derive(Deserialize)]
struct TaskStatus {
    status: String,
}
#[derive(Deserialize)]
struct SearchHit {
    id: Uuid,
}
#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}
#[derive(Serialize)]
struct SearchDocument<'a> {
    id: Uuid,
    organization_id: Uuid,
    filename: &'a str,
}

impl FileSearch {
    pub fn new(settings: SearchSettings) -> ApplicationResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(ApplicationError::internal)?;
        Ok(Self { client, settings })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> ApplicationResult<T> {
        let url = self
            .settings
            .endpoint
            .join(path)
            .map_err(ApplicationError::internal)?;
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(self.settings.api_key.expose());
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| ApplicationError::IntegrationUnavailable)?;
        if !response.status().is_success() {
            return Err(ApplicationError::IntegrationUnavailable);
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ApplicationError::IntegrationUnavailable)?;
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err(ApplicationError::IntegrationUnavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| ApplicationError::IntegrationUnavailable)
    }

    async fn wait_for_task(&self, submitted: SubmittedTask) -> ApplicationResult<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let task: TaskStatus = self
                .request(Method::GET, &format!("tasks/{}", submitted.task_uid), None)
                .await?;
            match task.status.as_str() {
                "succeeded" => return Ok(()),
                "failed" | "canceled" => return Err(ApplicationError::IntegrationUnavailable),
                "enqueued" | "processing" => {}
                _ => return Err(ApplicationError::IntegrationUnavailable),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ApplicationError::IntegrationUnavailable);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn configure_index(&self) -> ApplicationResult<()> {
        let url = self
            .settings
            .endpoint
            .join(&format!("indexes/{}", self.settings.index))
            .map_err(ApplicationError::internal)?;
        let response = self
            .client
            .get(url)
            .bearer_auth(self.settings.api_key.expose())
            .send()
            .await
            .map_err(|_| ApplicationError::IntegrationUnavailable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            let task = self
                .request(
                    Method::POST,
                    "indexes",
                    Some(serde_json::json!({"uid": self.settings.index, "primaryKey": "id"})),
                )
                .await?;
            self.wait_for_task(task).await?;
        } else if response.status().is_success() {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct IndexDescription {
                primary_key: Option<String>,
            }
            let index: IndexDescription = self
                .request(
                    Method::GET,
                    &format!("indexes/{}", self.settings.index),
                    None,
                )
                .await?;
            if index.primary_key.as_deref() != Some("id") {
                return Err(ApplicationError::InvalidInput(
                    "search index must use id as its primary key",
                ));
            }
        } else {
            return Err(ApplicationError::IntegrationUnavailable);
        }
        let task = self.request(Method::PATCH, &format!("indexes/{}/settings", self.settings.index),
            Some(serde_json::json!({"searchableAttributes": ["filename"], "filterableAttributes": ["organization_id"], "displayedAttributes": ["id"]}))).await?;
        self.wait_for_task(task).await
    }

    pub async fn synchronize(&self, file: &file::Model) -> ApplicationResult<()> {
        let task = if file.state == "ready" {
            self.request(
                Method::POST,
                &format!("indexes/{}/documents?primaryKey=id", self.settings.index),
                Some(
                    serde_json::to_value([SearchDocument {
                        id: file.id,
                        organization_id: file.organization_id,
                        filename: &file.filename,
                    }])
                    .map_err(ApplicationError::internal)?,
                ),
            )
            .await?
        } else {
            self.request(
                Method::DELETE,
                &format!("indexes/{}/documents/{}", self.settings.index, file.id),
                None,
            )
            .await?
        };
        self.wait_for_task(task).await
    }

    pub async fn candidates(
        &self,
        organization_id: Uuid,
        query: &str,
    ) -> ApplicationResult<Vec<Uuid>> {
        if query.is_empty() || query.chars().count() > 200 {
            return Err(ApplicationError::InvalidInput(
                "search query must contain 1 to 200 characters",
            ));
        }
        let response: SearchResponse = self.request(Method::POST, &format!("indexes/{}/search", self.settings.index),
            Some(serde_json::json!({"q": query, "filter": format!("organization_id = \"{organization_id}\""), "limit": 100, "attributesToRetrieve": ["id"]}))).await?;
        Ok(response
            .hits
            .into_iter()
            .take(100)
            .map(|hit| hit.id)
            .collect())
    }
}
