mod permissions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("MIGRATION_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL"))?;
    let mut options = sea_orm::ConnectOptions::new(url);
    options.max_connections(1).sqlx_logging(false);
    let db = sea_orm::Database::connect(options).await?;
    migration::migrate(&db).await?;
    permissions::configure(&db).await?;
    db.close().await?;
    Ok(())
}
