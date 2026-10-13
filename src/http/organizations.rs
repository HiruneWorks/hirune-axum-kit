use super::{
    HttpState,
    contracts::*,
    error::ApiError,
    extract::{ApiJson, ApiPath, ApiQuery, Session},
};
use axum::{Json, extract::State, http::StatusCode};
use uuid::Uuid;

#[utoipa::path(post, path="/api/v1/organizations", tag="Organizations", security(("cookieAuth"=[]),("bearerAuth"=[])), request_body=OrganizationInput, responses((status=201,body=OrganizationOutput)))]
pub async fn create_organization(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiJson(input): ApiJson<OrganizationInput>,
) -> Result<(StatusCode, Json<OrganizationOutput>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .app
                .organizations
                .create_organization(&actor, input.name)
                .await?
                .into(),
        ),
    ))
}
#[utoipa::path(get, path="/api/v1/organizations", tag="Organizations", security(("cookieAuth"=[]),("bearerAuth"=[])), params(PaginationQuery), responses((status=200,body=OrganizationPageOutput)))]
pub async fn list_organizations(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiQuery(query): ApiQuery<PaginationQuery>,
) -> Result<Json<OrganizationPageOutput>, ApiError> {
    let page = state
        .app
        .organizations
        .list_organizations(&actor, query.limit, query.cursor.as_deref())
        .await?;
    Ok(Json(OrganizationPageOutput {
        items: page.items.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
    }))
}
#[utoipa::path(get, path="/api/v1/organizations/{organization_id}", tag="Organizations", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path)), responses((status=200,body=OrganizationOutput)))]
pub async fn get_organization(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
) -> Result<Json<OrganizationOutput>, ApiError> {
    Ok(Json(
        state
            .app
            .organizations
            .get_organization(&actor, organization_id)
            .await?
            .into(),
    ))
}
#[utoipa::path(put, path="/api/v1/organizations/{organization_id}", tag="Organizations", security(("cookieAuth"=[]),("bearerAuth"=[])), params(("organization_id"=Uuid,Path)), request_body=RenameOrganizationInput, responses((status=200,body=OrganizationOutput)))]
pub async fn rename_organization(
    State(state): State<HttpState>,
    Session(actor): Session,
    ApiPath(organization_id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<RenameOrganizationInput>,
) -> Result<Json<OrganizationOutput>, ApiError> {
    Ok(Json(
        state
            .app
            .organizations
            .rename_organization(&actor, organization_id, input.expected_version, input.name)
            .await?
            .into(),
    ))
}
