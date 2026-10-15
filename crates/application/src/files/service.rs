use super::{
    FileContent, FileSettings, FileSummary, ReserveFile, TransferStream, entities::file,
    repository, search::FileSearch, storage::FileStorage, transfer,
};
use crate::{
    ApplicationError, ApplicationResult,
    database::database_time,
    identity::{AuthenticatedSession, IdentityService},
    organizations::OrganizationService,
    pagination::{Page, PageRequest},
};
use chrono::Duration;
use futures_util::StreamExt;
use jobs::TraceContext;
use sea_orm::{DatabaseTransaction, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::{ops::Range, sync::Arc};
use tokio::sync::Semaphore;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub const FILE_DELETE_JOB: &str = "files.delete_original";
pub const FILE_SEARCH_JOB: &str = "files.synchronize_search";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileJob {
    pub file_id: Uuid,
}

#[derive(Clone)]
pub struct FileService {
    pub(super) identity: IdentityService,
    pub(super) organizations: OrganizationService,
    pub(super) storage: Option<FileStorage>,
    pub(super) search: Option<FileSearch>,
    pub(super) settings: Arc<FileSettings>,
    transfer_permits: Arc<Semaphore>,
}

impl FileService {
    pub fn new(
        identity: IdentityService,
        organizations: OrganizationService,
        settings: FileSettings,
    ) -> ApplicationResult<Self> {
        settings.validate()?;
        Ok(Self {
            storage: settings.storage.as_ref().map(FileStorage::new),
            search: settings.search.clone().map(FileSearch::new).transpose()?,
            identity,
            organizations,
            transfer_permits: Arc::new(Semaphore::new(settings.transfer_concurrency)),
            settings: Arc::new(settings),
        })
    }

    pub fn upload_timeout_seconds(&self) -> u64 {
        self.settings.transfer_seconds + 65
    }

    pub async fn initialize_integrations(&self) -> ApplicationResult<()> {
        if let Some(storage) = &self.storage {
            storage.create_bucket().await?;
        }
        if let Some(search) = &self.search {
            search.configure_index().await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(account_id = %actor.account_id(), organization_id = %organization_id, request_id = %trace.request_id))]
    pub async fn reserve_file(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        input: ReserveFile,
        trace: TraceContext,
    ) -> ApplicationResult<FileSummary> {
        self.require_storage()?;
        validate_reservation(&input, &self.settings)?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let reserved = repository::reserved_bytes(&transaction, organization_id).await? as u64;
        let now = database_time(&transaction).await?;
        let (live, pending, recent) = repository::reservation_counts(
            &transaction,
            organization_id,
            actor.account_id(),
            now - Duration::minutes(1),
        )
        .await?;
        if recent >= self.settings.reservations_per_minute {
            return Err(ApplicationError::RateLimited);
        }
        if live >= self.settings.organization_file_limit
            || pending >= self.settings.pending_file_limit
        {
            return Err(ApplicationError::FileQuotaExceeded);
        }
        if reserved
            .checked_add(input.byte_size)
            .is_none_or(|bytes| bytes > self.settings.organization_quota_bytes)
        {
            return Err(ApplicationError::FileQuotaExceeded);
        }
        let expires_at =
            now + Duration::seconds(900.max(self.settings.transfer_seconds as i64 + 120));
        let file = repository::insert_reservation(
            &transaction,
            organization_id,
            actor.account_id(),
            input,
            expires_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(file.into())
    }

    #[tracing::instrument(skip_all, fields(file_id = %file_id, request_id = %trace.request_id))]
    pub async fn upload_original(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        file_id: Uuid,
        body: TransferStream,
        trace: TraceContext,
    ) -> ApplicationResult<FileSummary> {
        let storage = self.require_storage()?;
        let _permit = transfer::acquire_transfer(&self.transfer_permits)?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let file = require_scoped_file(
            repository::lock(&transaction, file_id).await?,
            organization_id,
        )?;
        let now = database_time(&transaction).await?;
        if file.uploader_id != actor.account_id() {
            return Err(ApplicationError::Forbidden);
        }
        if file.state != "uploading" || file.upload_expires_at <= now {
            return Err(ApplicationError::Conflict);
        }
        let transferring =
            repository::set_state(&transaction, file, "transferring", now, None).await?;
        transaction.commit().await?;
        let (temporary, content_hash) =
            transfer::spool_original(body, transferring.byte_size as u64, &self.settings).await?;
        storage
            .put_original(
                &transferring.object_key,
                temporary.path(),
                transferring.byte_size as u64,
            )
            .await?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let current = require_scoped_file(
            repository::lock(&transaction, file_id).await?,
            organization_id,
        )?;
        let now = database_time(&transaction).await?;
        if current.state != "transferring"
            || current.version != transferring.version
            || current.upload_expires_at <= now
        {
            return Err(ApplicationError::Conflict);
        }
        let ready =
            repository::set_state(&transaction, current, "ready", now, Some(content_hash)).await?;
        self.enqueue_search(&transaction, file_id, trace).await?;
        transaction.commit().await?;
        Ok(ready.into())
    }

    pub async fn get_file(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        file_id: Uuid,
    ) -> ApplicationResult<FileSummary> {
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let file = require_scoped_file(
            repository::find(&transaction, file_id).await?,
            organization_id,
        )?;
        if file.state != "ready" && file.uploader_id != actor.account_id() {
            return Err(ApplicationError::NotFound);
        }
        if matches!(file.state.as_str(), "deleting" | "deleted") {
            return Err(ApplicationError::NotFound);
        }
        transaction.commit().await?;
        Ok(file.into())
    }

    pub async fn list_files(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        limit: Option<u64>,
        cursor: Option<&str>,
    ) -> ApplicationResult<Page<FileSummary>> {
        let scope = format!("files:{organization_id}:{}", actor.account_id());
        let page = PageRequest::parse(limit, cursor, &scope)?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let files = repository::list_ready(&transaction, organization_id, page).await?;
        transaction.commit().await?;
        Ok(Page::from_rows(
            files.into_iter().map(Into::into).collect(),
            page,
            &scope,
            |file: &FileSummary| file.id,
        ))
    }

    pub async fn download_original(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        file_id: Uuid,
        range: Option<Range<u64>>,
    ) -> ApplicationResult<FileContent> {
        let storage = self.require_storage()?;
        let permit = transfer::acquire_transfer(&self.transfer_permits)?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let file = require_scoped_file(
            repository::find(&transaction, file_id).await?,
            organization_id,
        )?;
        if file.state != "ready" {
            return Err(ApplicationError::NotFound);
        }
        transaction.commit().await?;
        let range = range.unwrap_or(0..file.byte_size as u64);
        if range.start > range.end || range.end > file.byte_size as u64 {
            return Err(ApplicationError::InvalidInput("invalid file range"));
        }
        let byte_length = range.end - range.start;
        let body = storage.get_original(&file.object_key, range).await?;
        let stream = ReaderStream::with_capacity(body.into_async_read(), 64 * 1024).boxed();
        Ok(FileContent {
            file: file.into(),
            byte_length,
            stream: transfer::bounded_download(stream, permit, byte_length, &self.settings),
        })
    }

    pub async fn original_metadata(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        file_id: Uuid,
    ) -> ApplicationResult<FileSummary> {
        let file = self.get_file(actor, organization_id, file_id).await?;
        if file.state != "ready" {
            return Err(ApplicationError::NotFound);
        }
        self.require_storage()?
            .require_original_size(
                &format!("originals/{organization_id}/{file_id}"),
                file.byte_size,
            )
            .await?;
        Ok(file)
    }

    #[tracing::instrument(skip_all, fields(file_id = %file_id, request_id = %trace.request_id))]
    pub async fn delete_file(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        file_id: Uuid,
        trace: TraceContext,
    ) -> ApplicationResult<()> {
        let transaction = self.identity.database.begin().await?;
        let organization = self
            .organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let file = require_scoped_file(
            repository::lock(&transaction, file_id).await?,
            organization_id,
        )?;
        if file.uploader_id != actor.account_id() && organization.owner_id != actor.account_id() {
            return Err(ApplicationError::Forbidden);
        }
        if !matches!(file.state.as_str(), "deleting" | "deleted") {
            let now = database_time(&transaction).await?;
            repository::set_state(&transaction, file, "deleting", now, None).await?;
            jobs::enqueue(
                &transaction,
                Uuid::now_v7(),
                FILE_DELETE_JOB,
                &FileJob { file_id },
                trace,
            )
            .await?;
            self.enqueue_search(&transaction, file_id, trace).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn search_files(
        &self,
        actor: &AuthenticatedSession,
        organization_id: Uuid,
        query: &str,
    ) -> ApplicationResult<Vec<FileSummary>> {
        let search = self
            .search
            .as_ref()
            .ok_or(ApplicationError::IntegrationUnavailable)?;
        self.organizations
            .get_organization(actor, organization_id)
            .await?;
        let candidates = search.candidates(organization_id, query).await?;
        let transaction = self.identity.database.begin().await?;
        self.organizations
            .require_member_in_transaction(&transaction, actor, organization_id)
            .await?;
        let mut files =
            repository::ready_candidates(&transaction, organization_id, candidates.clone()).await?;
        transaction.commit().await?;
        files.sort_by_key(|file| {
            candidates
                .iter()
                .position(|id| *id == file.id)
                .unwrap_or(usize::MAX)
        });
        Ok(files.into_iter().take(20).map(Into::into).collect())
    }

    pub(super) async fn enqueue_search(
        &self,
        transaction: &DatabaseTransaction,
        file_id: Uuid,
        trace: TraceContext,
    ) -> ApplicationResult<()> {
        if self.search.is_some() {
            use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
            file::Entity::update_many()
                .col_expr(
                    file::Column::SearchDocumentMayExist,
                    sea_orm::sea_query::Expr::value(true),
                )
                .filter(file::Column::Id.eq(file_id))
                .filter(file::Column::State.eq("ready"))
                .exec(transaction)
                .await?;
            jobs::enqueue(
                transaction,
                Uuid::now_v7(),
                FILE_SEARCH_JOB,
                &FileJob { file_id },
                trace,
            )
            .await?;
        }
        Ok(())
    }

    pub(super) fn require_storage(&self) -> ApplicationResult<&FileStorage> {
        self.storage
            .as_ref()
            .ok_or(ApplicationError::IntegrationUnavailable)
    }
}

fn require_scoped_file(
    file: Option<file::Model>,
    organization_id: Uuid,
) -> ApplicationResult<file::Model> {
    file.filter(|file| file.organization_id == organization_id)
        .ok_or(ApplicationError::NotFound)
}

fn validate_reservation(input: &ReserveFile, settings: &FileSettings) -> ApplicationResult<()> {
    if input.byte_size > settings.max_file_bytes {
        return Err(ApplicationError::FileTooLarge);
    }
    super::validation::validate_filename(&input.filename)?;
    if let Some(media_type) = &input.media_type {
        super::validation::validate_media_type(media_type)?;
    }
    Ok(())
}
