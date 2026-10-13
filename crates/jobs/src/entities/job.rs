use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub kind: String,
    pub payload: Json,
    pub request_id: Uuid,
    pub state: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTimeUtc,
    pub claim_token: Option<Uuid>,
    pub lease_until: Option<DateTimeUtc>,
    pub deadline_at: Option<DateTimeUtc>,
    pub last_error: Option<String>,
    pub created_at: DateTimeUtc,
    pub finished_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
