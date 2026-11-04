use sqlx::{Row, SqlitePool};

use crate::models::simulation::{SimulationGeneration, SimulationRequest, StoredSimulation};

pub async fn save(
    pool: &SqlitePool,
    request: &SimulationRequest,
    history: &[SimulationGeneration],
) -> Result<i64, sqlx::Error> {
    let request_json =
        serde_json::to_string(request).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let mut transaction = pool.begin().await?;
    let id = sqlx::query("INSERT INTO simulations (request_json) VALUES (?)")
        .bind(request_json)
        .execute(&mut *transaction)
        .await?
        .last_insert_rowid();

    for snapshot in history {
        let generation = i64::try_from(snapshot.generation)
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        let world_json = serde_json::to_string(&snapshot.world)
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        sqlx::query(
            "INSERT INTO simulation_generations (simulation_id, generation, world_json) \
             VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(generation)
        .bind(world_json)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;
    Ok(id)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<StoredSimulation>, sqlx::Error> {
    let row = sqlx::query("SELECT created_at, request_json FROM simulations WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let request_json: String = row.try_get("request_json")?;
    let request = serde_json::from_str(&request_json)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let rows = sqlx::query(
        "SELECT generation, world_json FROM simulation_generations \
         WHERE simulation_id = ? ORDER BY generation",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut history = Vec::new();
    for row in &rows {
        history.push(read_generation(row)?);
    }

    Ok(Some(StoredSimulation {
        id,
        created_at: row.try_get("created_at")?,
        request,
        history,
    }))
}

pub async fn get_generation(
    pool: &SqlitePool,
    id: i64,
    generation: i64,
) -> Result<Option<SimulationGeneration>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT generation, world_json FROM simulation_generations \
         WHERE simulation_id = ? AND generation = ?",
    )
    .bind(id)
    .bind(generation)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => Ok(Some(read_generation(&row)?)),
        None => Ok(None),
    }
}

fn read_generation(row: &sqlx::sqlite::SqliteRow) -> Result<SimulationGeneration, sqlx::Error> {
    let generation: i64 = row.try_get("generation")?;
    let generation =
        usize::try_from(generation).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let world_json: String = row.try_get("world_json")?;
    let world =
        serde_json::from_str(&world_json).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    Ok(SimulationGeneration { generation, world })
}
