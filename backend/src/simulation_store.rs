use sqlx::{Row, SqlitePool};

use crate::models::simulation::{
    SimulationCheckpoint, SimulationGeneration, SimulationListQuery, SimulationListResponse,
    SimulationRequest, SimulationSort, SimulationSummary, SortOrder, StoredSimulation,
};

pub async fn list(
    pool: &SqlitePool,
    query: &SimulationListQuery,
) -> Result<SimulationListResponse, sqlx::Error> {
    let sort = match query.sort {
        SimulationSort::CreatedAt => "created_at",
        SimulationSort::Name => "name COLLATE NOCASE",
        SimulationSort::Generations => "generations",
    };
    let order = match query.order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };
    // Only the enum values above enter the SQL. User input is bound separately.
    let sql = format!(
        "SELECT id, created_at, \
         json_extract(request_json, '$.automaton.name') AS name, \
         json_array_length(request_json, '$.initial_world.cells') AS height, \
         json_array_length(request_json, '$.initial_world.cells[0]') AS width, \
         json_extract(request_json, '$.generations') AS generations, \
         json_extract(request_json, '$.save_history') AS save_history \
         FROM simulations \
         WHERE instr(lower(json_extract(request_json, '$.automaton.name')), lower(?)) > 0 \
         ORDER BY {sort} {order}, id {order} LIMIT ? OFFSET ?"
    );
    let search = query.search.trim();
    let offset = i64::from(query.page - 1) * i64::from(query.page_size);
    let mut transaction = pool.begin().await?;
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM simulations \
         WHERE instr(lower(json_extract(request_json, '$.automaton.name')), lower(?)) > 0",
    )
    .bind(search)
    .fetch_one(&mut *transaction)
    .await?;
    let rows = sqlx::query(&sql)
        .bind(search)
        .bind(i64::from(query.page_size))
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;

    let mut items = Vec::new();
    for row in rows {
        items.push(SimulationSummary {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            created_at: row.try_get("created_at")?,
            width: read_size(&row, "width")?,
            height: read_size(&row, "height")?,
            generations: read_size(&row, "generations")?,
            save_history: row.try_get("save_history")?,
        });
    }
    Ok(SimulationListResponse {
        items,
        total,
        page: query.page,
        page_size: query.page_size,
    })
}

fn read_size(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<usize, sqlx::Error> {
    let value: i64 = row.try_get(column)?;
    usize::try_from(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

pub async fn save(
    pool: &SqlitePool,
    request: &SimulationRequest,
    history: &[SimulationGeneration],
) -> Result<i64, sqlx::Error> {
    save_with_checkpoint(pool, request, history, None).await
}

pub async fn save_with_checkpoint(
    pool: &SqlitePool,
    request: &SimulationRequest,
    history: &[SimulationGeneration],
    checkpoint: Option<&SimulationCheckpoint>,
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

    if let Some(checkpoint) = checkpoint {
        let state_json = serde_json::to_string(checkpoint)
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        sqlx::query("INSERT INTO simulation_checkpoints (simulation_id, state_json) VALUES (?, ?)")
            .bind(id)
            .bind(state_json)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(id)
}

pub struct StoredStep {
    pub request: SimulationRequest,
    pub request_json: String,
    pub current: SimulationGeneration,
    pub checkpoint: Option<SimulationCheckpoint>,
}

pub async fn get_step(pool: &SqlitePool, id: i64) -> Result<Option<StoredStep>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT s.request_json, g.generation, g.world_json, c.state_json \
         FROM simulations s JOIN simulation_generations g ON g.simulation_id = s.id \
         LEFT JOIN simulation_checkpoints c ON c.simulation_id = s.id \
         WHERE s.id = ? ORDER BY g.generation DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let request_json: String = row.try_get("request_json")?;
    let state_json: Option<String> = row.try_get("state_json")?;
    let request = serde_json::from_str(&request_json)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let checkpoint = state_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    Ok(Some(StoredStep {
        request,
        request_json,
        current: read_generation(&row)?,
        checkpoint,
    }))
}

pub async fn append_generation(
    pool: &SqlitePool,
    id: i64,
    previous_request_json: &str,
    request: &SimulationRequest,
    snapshot: &SimulationGeneration,
    checkpoint: &SimulationCheckpoint,
) -> Result<bool, sqlx::Error> {
    let request_json =
        serde_json::to_string(request).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let world_json = serde_json::to_string(&snapshot.world)
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let state_json =
        serde_json::to_string(checkpoint).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let generation =
        i64::try_from(snapshot.generation).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let mut transaction = pool.begin().await?;
    // Only one request may advance a given saved state.
    let updated =
        sqlx::query("UPDATE simulations SET request_json = ? WHERE id = ? AND request_json = ?")
            .bind(request_json)
            .bind(id)
            .bind(previous_request_json)
            .execute(&mut *transaction)
            .await?;
    if updated.rows_affected() == 0 {
        return Ok(false);
    }
    if !request.save_history {
        sqlx::query("DELETE FROM simulation_generations WHERE simulation_id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("INSERT INTO simulation_generations (simulation_id, generation, world_json) VALUES (?, ?, ?)")
        .bind(id).bind(generation).bind(world_json).execute(&mut *transaction).await?;
    sqlx::query(
        "INSERT INTO simulation_checkpoints (simulation_id, state_json) VALUES (?, ?) \
                 ON CONFLICT(simulation_id) DO UPDATE SET state_json = excluded.state_json",
    )
    .bind(id)
    .bind(state_json)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(true)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    // Foreign keys delete generations and the continuation checkpoint atomically.
    let result = sqlx::query("DELETE FROM simulations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
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
