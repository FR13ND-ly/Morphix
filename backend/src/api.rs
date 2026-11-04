use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    config::state::AppState,
    models::simulation::{
        ErrorResponse, SimulationGeneration, SimulationRequest, SimulationResponse,
        SimulationValidationResponse, StoredSimulation,
    },
    simulation_store, simulator,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/simulations/validate", post(validate_simulation))
        .route("/simulations/run", post(run_simulation))
        .route("/simulations/{id}", get(get_simulation))
        .route(
            "/simulations/{id}/generations/{generation}",
            get(get_generation),
        )
        .layer(DefaultBodyLimit::max(256 * 1024))
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

    let (request, history) = tokio::task::spawn_blocking(move || {
        let history = simulator::run(
            &request.automaton,
            &request.initial_world,
            request.generations,
            request.seed,
            request.save_history,
        )?;
        Ok::<_, String>((request, history))
    })
    .await
    .map_err(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Simulation worker could not complete".to_owned(),
        )
    })?
    .map_err(|error| error_response(StatusCode::UNPROCESSABLE_ENTITY, error))?;

    let id = simulation_store::save(&state.db, &request, &history)
        .await
        .map_err(storage_error)?;

    Ok(Json(SimulationResponse {
        id,
        seed: request.seed,
        history,
    }))
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
