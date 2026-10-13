use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_flows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub state_hash: String,
    pub binding_hash: String,
    pub provider: String,
    pub secret: String,
    pub account_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub security_version: Option<i64>,
    pub expires_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
