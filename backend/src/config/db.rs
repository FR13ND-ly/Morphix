use std::{str::FromStr, time::Duration};

use sqlx::{SqlitePool, sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};

pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?;

    let mut transaction = pool.begin().await?;
    sqlx::raw_sql(include_str!("../../migrations/0001_create_simulations.sql"))
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(pool)
}
