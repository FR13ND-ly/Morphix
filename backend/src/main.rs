mod config;
mod models;
mod engine;
mod presets;

use axum::{Json, routing::post};

use models::simulation::{
    SimulationRequest,
    SimulationResult,
};
use axum::{Router, extract::State, http::StatusCode, routing::get};
use config::{Config, db, state::AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err(error.into());
    }

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/simulations", post(simulate))
        .route(
            "/api/examples/game-of-life",
            get(|| async { Json(presets::game_of_life()) }),
        )
        .route(
            "/api/examples/sandpile",
            get(|| async { Json(presets::sandpile()) }),
        )
        .with_state(AppState { db: pool.clone() });
    let listener = tokio::net::TcpListener::bind(config.address).await?;

    println!("Listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    pool.close().await;
    Ok(())
}

async fn simulate(
    Json(request): Json<SimulationRequest>,
) -> Result<Json<SimulationResult>, (StatusCode, &'static str)> {
    let result = tokio::task::spawn_blocking(move || {
        engine::simulator::run(request)
    })
    .await
    .map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Simulation failed")
    })?
    .map_err(|message| {
        (StatusCode::UNPROCESSABLE_ENTITY, message)
    })?;

    Ok(Json(result))
}

async fn health(State(state): State<AppState>) -> Result<&'static str, StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok("ok")
}
