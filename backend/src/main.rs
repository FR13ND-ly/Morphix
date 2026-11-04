use axum::{Router, extract::State, http::StatusCode, routing::get};
use backend::{
    api,
    config::{Config, db, state::AppState},
};

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
        .merge(api::router())
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

async fn health(State(state): State<AppState>) -> Result<&'static str, StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok("ok")
}
