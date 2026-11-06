use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    config::state::AppState,
    models::simulation::{
        ErrorResponse, SimulationGeneration, SimulationListQuery, SimulationListResponse,
        SimulationRequest, SimulationResponse, SimulationStepRequest, SimulationValidationResponse,
        StoredSimulation,
    },
    simulation_store, simulator,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/simulations", get(list_simulations))
        .route("/simulations/validate", post(validate_simulation))
        .route("/simulations/run", post(run_simulation))
        .route("/simulations/examples/{name}", get(get_example))
        .route(
            "/simulations/{id}",
            get(get_simulation).delete(delete_simulation),
        )
        .route("/simulations/{id}/step", post(step_simulation))
        .route(
            "/simulations/{id}/generations/{generation}",
            get(get_generation),
        )
        .layer(DefaultBodyLimit::max(256 * 1024))
}

async fn list_simulations(
    State(state): State<AppState>,
    query: Result<Query<SimulationListQuery>, QueryRejection>,
) -> Result<Json<SimulationListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let Query(query) = query.map_err(|error| error_response(error.status(), error.body_text()))?;
    if query.page == 0 || !(1..=100).contains(&query.page_size) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "page must be at least 1 and page_size must be between 1 and 100".to_owned(),
        ));
    }
    let result = simulation_store::list(&state.db, &query)
        .await
        .map_err(storage_error)?;
    Ok(Json(result))
}

async fn get_example(
    Path(name): Path<String>,
) -> Result<Json<SimulationRequest>, (StatusCode, Json<ErrorResponse>)> {
    let source = match name.as_str() {
        "game-of-life" => include_str!("../examples/game-of-life.json"),
        "forest-fire" => include_str!("../examples/forest-fire.json"),
        "diffusion" => include_str!("../examples/diffusion.json"),
        "artificial-ecosystem" => include_str!("../examples/artificial-ecosystem.json"),
        _ => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "Example not found".to_owned(),
            ));
        }
    };
    let request = serde_json::from_str(source).map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Example could not be loaded".to_owned(),
        )
    })?;
    Ok(Json(request))
}

async fn validate_simulation(
    payload: Result<Json<SimulationRequest>, JsonRejection>,
) -> Result<Json<SimulationValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let Json(request) =
        payload.map_err(|error| error_response(error.status(), error.body_text()))?;
    request
        .validate_limits()
        .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;

    tokio::task::spawn_blocking(move || {
        request
            .automaton
            .validate_world(&request.initial_world)
            .map_err(|error| format!("Generation 0: {error}"))
    })
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Validation worker could not complete".to_owned(),
        )
    })?
    .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;

    Ok(Json(SimulationValidationResponse { valid: true }))
}

async fn run_simulation(
    State(state): State<AppState>,
    payload: Result<Json<SimulationRequest>, JsonRejection>,
) -> Result<Json<SimulationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let Json(request) =
        payload.map_err(|error| error_response(error.status(), error.body_text()))?;
    request
        .validate_limits()
        .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;

    let (request, history, checkpoint) = tokio::task::spawn_blocking(move || {
        let (history, checkpoint) = simulator::run_with_checkpoint(
            &request.automaton,
            &request.initial_world,
            request.generations,
            request.seed,
            request.save_history,
        )?;
        Ok::<_, String>((request, history, checkpoint))
    })
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Simulation worker could not complete".to_owned(),
        )
    })?
    .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;

    let id =
        simulation_store::save_with_checkpoint(&state.db, &request, &history, Some(&checkpoint))
            .await
            .map_err(storage_error)?;

    Ok(Json(SimulationResponse {
        id,
        seed: request.seed,
        history,
    }))
}

async fn step_simulation(
    State(state): State<AppState>,
    path: Result<Path<i64>, PathRejection>,
    payload: Result<Json<SimulationStepRequest>, JsonRejection>,
) -> Result<Json<SimulationGeneration>, (StatusCode, Json<ErrorResponse>)> {
    let Path(id) = path.map_err(|error| error_response(error.status(), error.body_text()))?;
    let Json(expected) =
        payload.map_err(|error| error_response(error.status(), error.body_text()))?;
    let stored = simulation_store::get_step(&state.db, id)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "Simulation not found".to_owned()))?;
    if stored.current.generation != expected.generation {
        return Err(step_conflict());
    }
    let (mut request, previous_json, next, checkpoint) = tokio::task::spawn_blocking(move || {
        let request = stored.request;
        // Older records have no checkpoint: recover it once using their original seed.
        let checkpoint = match stored.checkpoint {
            Some(checkpoint) => checkpoint,
            None => {
                simulator::run_with_checkpoint(
                    &request.automaton,
                    &request.initial_world,
                    stored.current.generation,
                    request.seed,
                    false,
                )?
                .1
            }
        };
        let (next, checkpoint) = simulator::continue_step(
            &request.automaton,
            &stored.current,
            request.seed,
            checkpoint,
        )?;
        Ok::<_, String>((request, stored.request_json, next, checkpoint))
    })
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Simulation worker could not complete".to_owned(),
        )
    })?
    .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;
    request.generations = next.generation;
    let saved = simulation_store::append_generation(
        &state.db,
        id,
        &previous_json,
        &request,
        &next,
        &checkpoint,
    )
    .await
    .map_err(storage_error)?;
    if !saved {
        return Err(step_conflict());
    }
    Ok(Json(next))
}

fn step_conflict() -> (StatusCode, Json<ErrorResponse>) {
    error_response(
        StatusCode::CONFLICT,
        "Simulation has advanced in another request. Reload before continuing.".to_owned(),
    )
}

async fn get_simulation(
    State(state): State<AppState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<StoredSimulation>, (StatusCode, Json<ErrorResponse>)> {
    let Path(id) = path.map_err(|error| error_response(error.status(), error.body_text()))?;
    let simulation = simulation_store::get(&state.db, id)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "Simulation not found".to_owned()))?;
    Ok(Json(simulation))
}

async fn delete_simulation(
    State(state): State<AppState>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let Path(id) = path.map_err(|error| error_response(error.status(), error.body_text()))?;
    let deleted = simulation_store::delete(&state.db, id)
        .await
        .map_err(storage_error)?;
    if !deleted {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "Simulation not found".to_owned(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_generation(
    State(state): State<AppState>,
    path: Result<Path<(i64, i64)>, PathRejection>,
) -> Result<Json<SimulationGeneration>, (StatusCode, Json<ErrorResponse>)> {
    let Path((id, generation)) =
        path.map_err(|error| error_response(error.status(), error.body_text()))?;
    let snapshot = simulation_store::get_generation(&state.db, id, generation)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "Generation not found or not saved".to_owned(),
            )
        })?;
    Ok(Json(snapshot))
}

fn storage_error(error: sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
    eprintln!("Simulation storage error: {error}");
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Simulation storage operation failed".to_owned(),
    )
}

fn error_response(status: StatusCode, error: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error }))
}
