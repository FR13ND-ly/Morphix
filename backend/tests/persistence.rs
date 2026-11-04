use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use backend::{
    api,
    config::{db, state::AppState},
    models::{
        action::{Action, NeighborSelection},
        object_type::PropertyDefinition,
        simulation::{SimulationRequest, StoredSimulation},
    },
    simulation_store, simulator,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

fn life_request() -> Value {
    serde_json::from_str(include_str!("../examples/game-of-life.json")).unwrap()
}

#[tokio::test]
async fn saves_predator_prey_interactions_and_replays_the_loaded_configuration() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let request: Value =
        serde_json::from_str(include_str!("../examples/predator-prey.json")).unwrap();
    let (status, result) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let id = result["id"].as_i64().unwrap();
    let (status, loaded) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let stored: StoredSimulation = serde_json::from_value(loaded).unwrap();
    let request = stored.request;
    let replay = simulator::run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        request.save_history,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&stored.history).unwrap(),
        result["history"]
    );
    assert_eq!(serde_json::to_value(replay).unwrap(), result["history"]);
    pool.close().await;
}

async fn call(
    pool: &SqlitePool,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let response = api::router()
        .with_state(AppState { db: pool.clone() })
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.map(Value::to_string).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["content-type"], "application/json");
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn counts(pool: &SqlitePool) -> (i64, i64) {
    let simulations = sqlx::query_scalar("SELECT COUNT(*) FROM simulations")
        .fetch_one(pool)
        .await
        .unwrap();
    let generations = sqlx::query_scalar("SELECT COUNT(*) FROM simulation_generations")
        .fetch_one(pool)
        .await
        .unwrap();
    (simulations, generations)
}

#[tokio::test]
async fn saves_and_recovers_the_complete_configuration_and_numbered_history() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: Value =
        serde_json::from_str(include_str!("../examples/rule-30.json")).unwrap();
    request["generations"] = json!(12);
    request["seed"] = json!(u64::MAX);
    request["automaton"]["name"] = json!("Viață — exemplu școlar");
    let parsed: SimulationRequest = serde_json::from_value(request.clone()).unwrap();

    let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let id = run["id"].as_i64().unwrap();
    assert!(id > 0);

    let (status, saved) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["id"], id);
    assert!(saved["created_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(saved["request"], serde_json::to_value(parsed).unwrap());
    assert_eq!(saved["request"]["seed"].as_u64(), Some(u64::MAX));
    assert_eq!(saved["history"], run["history"]);
    for (generation, snapshot) in saved["history"].as_array().unwrap().iter().enumerate() {
        assert_eq!(snapshot["generation"], generation);
    }

    let (status, snapshot) = call(
        &pool,
        "GET",
        &format!("/simulations/{id}/generations/1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot, run["history"][1]);
    assert_eq!(counts(&pool).await, (1, 13));

    let (status, replay) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(replay["id"], run["id"]);
    assert_eq!(replay["history"], run["history"]);
    let (status, original) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(original, saved);
    assert_eq!(counts(&pool).await, (2, 26));
    pool.close().await;
}

#[tokio::test]
async fn final_only_runs_keep_the_initial_configuration_and_only_the_final_snapshot() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request = life_request();
    request["generations"] = json!(3);
    request["save_history"] = json!(false);

    let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let id = run["id"].as_i64().unwrap();
    let (status, saved) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["request"]["initial_world"], request["initial_world"]);
    assert_eq!(saved["request"]["save_history"], false);
    assert_eq!(saved["history"], run["history"]);
    assert_eq!(counts(&pool).await, (1, 1));

    for generation in [0, 1, 2, 4] {
        let (status, error) = call(
            &pool,
            "GET",
            &format!("/simulations/{id}/generations/{generation}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
        assert!(error["error"].is_string());
    }
    let (status, snapshot) = call(
        &pool,
        "GET",
        &format!("/simulations/{id}/generations/3"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot, run["history"][0]);
    pool.close().await;
}

#[tokio::test]
async fn restores_continuous_cell_properties_and_agents_without_losing_values_or_ids() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/moving-agents.json")).unwrap();
    let continuous = PropertyDefinition {
        default_value: 0.0,
        min_value: -100.0,
        max_value: 100.0,
        integer_only: false,
    };
    request.automaton.object_types[0]
        .properties
        .insert("temperature".to_owned(), continuous.clone());
    request.automaton.object_types[1]
        .properties
        .insert("energy".to_owned(), continuous);
    for row in &mut request.initial_world.cells {
        for cell in row {
            cell.properties
                .insert("temperature".to_owned(), 37.86405596920032);
        }
    }
    for agent in &mut request.initial_world.agents {
        agent
            .state
            .properties
            .insert("energy".to_owned(), 0.8455124082255701);
    }
    request.initial_world.agents[0].id = u64::MAX;
    request.automaton.rules[0].actions = vec![Action::MoveToFreeNeighbor {
        selection: NeighborSelection::Random,
        cell_type: Some(0),
        offsets: None,
    }];
    let history = simulator::run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        true,
    )
    .unwrap();

    let id = simulation_store::save(&pool, &request, &history)
        .await
        .unwrap();
    let restored = simulation_store::get(&pool, id).await.unwrap().unwrap();

    assert_eq!(
        serde_json::to_value(restored.request).unwrap(),
        serde_json::to_value(request).unwrap()
    );
    assert_eq!(
        serde_json::to_value(restored.history).unwrap(),
        serde_json::to_value(history).unwrap()
    );
    pool.close().await;
}

#[tokio::test]
async fn zero_step_runs_store_generation_zero_in_both_history_modes() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request = life_request();
    request["generations"] = json!(0);
    for save_history in [true, false] {
        request["save_history"] = json!(save_history);
        let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
        assert_eq!(status, StatusCode::OK);
        let id = run["id"].as_i64().unwrap();
        let (status, snapshot) = call(
            &pool,
            "GET",
            &format!("/simulations/{id}/generations/0"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snapshot["generation"], 0);
        assert_eq!(snapshot["world"], request["initial_world"]);
    }
    assert_eq!(counts(&pool).await, (2, 2));
    pool.close().await;
}

#[tokio::test]
async fn unknown_simulations_and_generations_return_json_not_found_errors() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    for path in ["/simulations/123", "/simulations/123/generations/0"] {
        let (status, error) = call(&pool, "GET", path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(error["error"].is_string());
    }
    pool.close().await;
}

#[tokio::test]
async fn invalid_id_and_generation_path_parameters_return_json_bad_request_errors() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    for path in [
        "/simulations/not-a-number",
        "/simulations/18446744073709551615",
        "/simulations/1/generations/not-a-number",
        "/simulations/1/generations/18446744073709551615",
    ] {
        let (status, error) = call(&pool, "GET", path, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert!(error["error"].is_string());
    }
    pool.close().await;
}

#[tokio::test]
async fn invalid_configurations_and_failed_simulations_leave_no_saved_records() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut invalid = life_request();
    invalid["initial_world"]["cells"][0][0]["type_id"] = json!(99);
    let mut execution_error = life_request();
    execution_error["automaton"]["rules"] = json!([{
        "target": "cell",
        "condition": { "kind": "always" },
        "probability": { "kind": "subtract", "args": [
            { "kind": "constant", "args": 1 },
            { "kind": "count_neighbors", "args": { "type_id": 1 } }
        ] },
        "actions": []
    }]);
    for (request, message) in [(invalid, "Generation 0"), (execution_error, "Generation 1")] {
        let (status, error) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
        assert!(error["error"].as_str().unwrap().contains(message));
        assert_eq!(counts(&pool).await, (0, 0));
    }
    pool.close().await;
}

#[tokio::test]
async fn a_failed_generation_insert_rolls_back_the_simulation_and_previous_inserts() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(
        "CREATE TRIGGER reject_generation BEFORE INSERT ON simulation_generations \
         WHEN NEW.generation = 1 BEGIN SELECT RAISE(ABORT, 'test insert failure'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (status, error) = call(&pool, "POST", "/simulations/run", Some(&life_request())).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        error,
        json!({ "error": "Simulation storage operation failed" })
    );
    assert_eq!(counts(&pool).await, (0, 0));
    pool.close().await;
}

#[tokio::test]
async fn unavailable_storage_returns_json_server_errors_for_runs_and_reads() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    pool.close().await;
    let request = life_request();
    for (method, path, body) in [
        ("POST", "/simulations/run", Some(&request)),
        ("GET", "/simulations/1", None),
        ("GET", "/simulations/1/generations/0", None),
    ] {
        let (status, error) = call(&pool, method, path, body).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error,
            json!({ "error": "Simulation storage operation failed" })
        );
    }
}

struct TemporaryDatabase(PathBuf);

impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn file_storage_survives_pool_closure_and_reinitialization() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let file = TemporaryDatabase(std::env::temp_dir().join(format!(
        "cellab-persistence-{}-{timestamp}.db",
        std::process::id()
    )));
    let url = format!("sqlite://{}", file.0.to_string_lossy().replace('\\', "/"));
    let pool = db::connect(&url).await.unwrap();
    let request = life_request();
    let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    pool.close().await;

    let reopened = db::connect(&url).await.unwrap();
    let id = run["id"].as_i64().unwrap();
    let (status, saved) = call(&reopened, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["history"], run["history"]);
    let parsed: SimulationRequest = serde_json::from_value(request).unwrap();
    assert_eq!(saved["request"], serde_json::to_value(parsed).unwrap());
    assert_eq!(counts(&reopened).await, (1, 3));
    reopened.close().await;
}
