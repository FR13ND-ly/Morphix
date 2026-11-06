mod common;

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
    let request: Value = serde_json::from_value(common::captures()).unwrap();
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
    if status != StatusCode::NO_CONTENT {
        assert_eq!(response.headers()["content-type"], "application/json");
    }
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    if status == StatusCode::NO_CONTENT {
        assert!(bytes.is_empty());
        (status, Value::Null)
    } else {
        (status, serde_json::from_slice(&bytes).unwrap())
    }
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

async fn save_list_fixture(pool: &SqlitePool, name: &str, generations: usize) -> i64 {
    let mut request: SimulationRequest = serde_json::from_value(life_request()).unwrap();
    request.automaton.name = name.to_owned();
    request.generations = generations;
    request.save_history = generations != 0;
    let history = simulator::run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        request.save_history,
    )
    .unwrap();
    simulation_store::save(pool, &request, &history)
        .await
        .unwrap()
}

#[tokio::test]
async fn stored_diffusion_history_preserves_decimal_values_exactly() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/diffusion.json")).unwrap();
    request.generations = 5;
    request.save_history = true;
    let history = simulator::run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        request.save_history,
    )
    .unwrap();
    let id = simulation_store::save(&pool, &request, &history)
        .await
        .unwrap();
    let saved = simulation_store::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&saved.history).unwrap(),
        serde_json::to_value(&history).unwrap()
    );
    pool.close().await;
}

#[tokio::test]
async fn lists_an_empty_database_with_default_pagination() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let (status, response) = call(&pool, "GET", "/simulations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        response,
        json!({"items": [], "total": 0, "page": 1, "page_size": 20})
    );
    pool.close().await;
}

#[tokio::test]
async fn lists_summaries_without_loading_configuration_or_history() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let id = save_list_fixture(&pool, "Viață — test", 0).await;
    let (status, response) = call(&pool, "GET", "/simulations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["total"], 1);
    let item = &response["items"][0];
    assert_eq!(item["id"], id);
    assert_eq!(item["name"], "Viață — test");
    assert_eq!(
        item["width"],
        life_request()["initial_world"]["cells"][0]
            .as_array()
            .unwrap()
            .len()
    );
    assert_eq!(
        item["height"],
        life_request()["initial_world"]["cells"]
            .as_array()
            .unwrap()
            .len()
    );
    assert_eq!(item["generations"], 0);
    assert_eq!(item["save_history"], false);
    assert!(item["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(item.get("request").is_none());
    assert!(item.get("history").is_none());
    let (status, saved) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["request"]["automaton"]["name"], item["name"]);
    assert_eq!(saved["history"][0]["generation"], 0);
    pool.close().await;
}

#[tokio::test]
async fn paginates_in_a_stable_order_and_sorts_names_and_generation_counts() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let alpha = save_list_fixture(&pool, "Alpha", 2).await;
    let beta = save_list_fixture(&pool, "Beta", 0).await;
    let gamma = save_list_fixture(&pool, "Gamma", 1).await;
    sqlx::query("UPDATE simulations SET created_at = '2026-01-01T00:00:00.000Z'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, first) = call(&pool, "GET", "/simulations?page_size=2", None).await;
    assert_eq!(first["total"], 3);
    assert_eq!(first["items"][0]["id"], gamma);
    assert_eq!(first["items"][1]["id"], beta);
    let (_, second) = call(&pool, "GET", "/simulations?page_size=2&page=2", None).await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["id"], alpha);
    let (_, names) = call(&pool, "GET", "/simulations?sort=name&order=asc", None).await;
    assert_eq!(names["items"][0]["id"], alpha);
    assert_eq!(names["items"][2]["id"], gamma);
    let (_, generations) = call(
        &pool,
        "GET",
        "/simulations?sort=generations&order=asc",
        None,
    )
    .await;
    assert_eq!(generations["items"][0]["id"], beta);
    assert_eq!(generations["items"][1]["id"], gamma);
    let (_, empty) = call(
        &pool,
        "GET",
        "/simulations?page=4294967295&page_size=100",
        None,
    )
    .await;
    assert_eq!(empty["total"], 3);
    assert_eq!(empty["items"], json!([]));
    pool.close().await;
}

#[tokio::test]
async fn searches_names_as_literal_text_and_counts_only_matches() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    save_list_fixture(&pool, "Game of Life", 1).await;
    let id = save_list_fixture(&pool, "Life 100%_done", 0).await;
    save_list_fixture(&pool, "Diffusion", 1).await;
    let (_, response) = call(
        &pool,
        "GET",
        "/simulations?search=%20LIFE%20&page_size=1",
        None,
    )
    .await;
    assert_eq!(response["total"], 2);
    assert_eq!(response["items"].as_array().unwrap().len(), 1);
    let (_, literal) = call(&pool, "GET", "/simulations?search=%25_", None).await;
    assert_eq!(literal["total"], 1);
    assert_eq!(literal["items"][0]["id"], id);
    let (_, injection) = call(&pool, "GET", "/simulations?search=%27%20OR%201%3D1--", None).await;
    assert_eq!(injection["total"], 0);
    assert_eq!(counts(&pool).await.0, 3);
    pool.close().await;
}

#[tokio::test]
async fn rejects_invalid_list_queries_with_json_errors() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    for query in [
        "page=0",
        "page=-1",
        "page=4294967296",
        "page_size=0",
        "page_size=101",
        "page_size=text",
        "sort=unknown",
        "sort=name%3BDROP",
        "order=unknown",
        "unexpected=1",
    ] {
        let (status, response) = call(&pool, "GET", &format!("/simulations?{query}"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {response}");
        assert!(!response["error"].as_str().unwrap().is_empty());
    }
    pool.close().await;
}

#[tokio::test]
async fn saves_and_recovers_the_complete_configuration_and_numbered_history() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: Value = serde_json::from_value(common::directional_cells()).unwrap();
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
    let mut request: SimulationRequest = serde_json::from_value(common::moving_agents()).unwrap();
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
        ("DELETE", "/simulations/1", None),
    ] {
        let (status, error) = call(&pool, method, path, body).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error,
            json!({ "error": "Simulation storage operation failed" })
        );
    }
}

#[tokio::test]
async fn deletes_only_the_selected_simulation_and_cascades_history_and_checkpoint() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let (_, first) = call(&pool, "POST", "/simulations/run", Some(&life_request())).await;
    let mut final_only = life_request();
    final_only["save_history"] = json!(false);
    let (_, second) = call(&pool, "POST", "/simulations/run", Some(&final_only)).await;
    let first_id = first["id"].as_i64().unwrap();
    let second_id = second["id"].as_i64().unwrap();
    let (status, response) = call(&pool, "DELETE", &format!("/simulations/{first_id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{response}");
    assert_eq!(counts(&pool).await, (1, 1));
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM simulation_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(checkpoints, 1);
    for path in [
        format!("/simulations/{first_id}"),
        format!("/simulations/{first_id}/generations/0"),
    ] {
        assert_eq!(
            call(&pool, "GET", &path, None).await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        call(
            &pool,
            "POST",
            &format!("/simulations/{first_id}/step"),
            Some(&json!({"generation":2}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, saved) = call(&pool, "GET", &format!("/simulations/{second_id}"), None).await;
    assert_eq!(saved["history"], second["history"]);
    let (_, list) = call(&pool, "GET", "/simulations", None).await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["id"], second_id);
    assert_eq!(
        call(&pool, "DELETE", &format!("/simulations/{second_id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(counts(&pool).await, (0, 0));
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM simulation_checkpoints")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(checkpoints, 0);
    pool.close().await;
}

#[tokio::test]
async fn a_step_started_before_deletion_cannot_recreate_the_simulation() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let (_, run) = call(&pool, "POST", "/simulations/run", Some(&life_request())).await;
    let id = run["id"].as_i64().unwrap();
    let stored = simulation_store::get_step(&pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        call(&pool, "DELETE", &format!("/simulations/{id}"), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (next, checkpoint) = simulator::continue_step(
        &stored.request.automaton,
        &stored.current,
        stored.request.seed,
        stored.checkpoint.unwrap(),
    )
    .unwrap();
    let mut request = stored.request;
    request.generations = next.generation;
    let appended = simulation_store::append_generation(
        &pool,
        id,
        &stored.request_json,
        &request,
        &next,
        &checkpoint,
    )
    .await
    .unwrap();
    assert!(!appended);
    assert_eq!(counts(&pool).await, (0, 0));
    assert_eq!(
        call(&pool, "DELETE", &format!("/simulations/{id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&pool, "DELETE", "/simulations/invalid-id", None)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    pool.close().await;
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
    let mut request: Value =
        serde_json::from_str(include_str!("../examples/forest-fire.json")).unwrap();
    request["generations"] = json!(2);
    let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    pool.close().await;

    let reopened = db::connect(&url).await.unwrap();
    let id = run["id"].as_i64().unwrap();
    let (status, saved) = call(&reopened, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["history"], run["history"]);
    let parsed: SimulationRequest = serde_json::from_value(request).unwrap();
    assert_eq!(saved["request"], serde_json::to_value(&parsed).unwrap());
    assert_eq!(counts(&reopened).await, (1, 3));
    let (status, next) = call(
        &reopened,
        "POST",
        &format!("/simulations/{id}/step"),
        Some(&json!({"generation":2})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["generation"], 3);
    let expected = simulator::run(
        &parsed.automaton,
        &parsed.initial_world,
        3,
        parsed.seed,
        true,
    )
    .unwrap();
    assert_eq!(next, serde_json::to_value(&expected[3]).unwrap());
    assert_eq!(counts(&reopened).await, (1, 4));
    let path = format!("/simulations/{id}/step");
    let body = json!({"generation":3});
    let (first, second) = tokio::join!(
        call(&reopened, "POST", &path, Some(&body)),
        call(&reopened, "POST", &path, Some(&body)),
    );
    let statuses = [first.0, second.0];
    assert!(statuses.contains(&StatusCode::OK), "{first:?} {second:?}");
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "{first:?} {second:?}"
    );
    assert_eq!(counts(&reopened).await, (1, 5));
    reopened.close().await;
}

#[tokio::test]
async fn probabilistic_steps_resume_from_sqlite_and_match_a_continuous_run() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/forest-fire.json")).unwrap();
    let expected = simulator::run(
        &request.automaton,
        &request.initial_world,
        12,
        request.seed,
        true,
    )
    .unwrap();
    request.generations = 0;
    let (status, run) = call(
        &pool,
        "POST",
        "/simulations/run",
        Some(&serde_json::to_value(&request).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let id = run["id"].as_i64().unwrap();
    for generation in 0..12 {
        let (status, next) = call(
            &pool,
            "POST",
            &format!("/simulations/{id}/step"),
            Some(&json!({"generation":generation})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        assert_eq!(
            next,
            serde_json::to_value(&expected[generation + 1]).unwrap()
        );
    }
    let saved = simulation_store::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(saved.request.generations, 12);
    assert_eq!(
        serde_json::to_value(saved.history).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let (status, list) = call(&pool, "GET", "/simulations", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"][0]["generations"], 12);
    pool.close().await;
}

#[tokio::test]
async fn old_records_recover_their_checkpoint_once_and_continue_probabilistic_rules() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/artificial-ecosystem.json")).unwrap();
    let expected = simulator::run(
        &request.automaton,
        &request.initial_world,
        request.generations + 2,
        request.seed,
        true,
    )
    .unwrap();
    let id = simulation_store::save(&pool, &request, &expected[..=request.generations])
        .await
        .unwrap();
    for generation in request.generations..request.generations + 2 {
        let (status, next) = call(
            &pool,
            "POST",
            &format!("/simulations/{id}/step"),
            Some(&json!({"generation":generation})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        assert_eq!(
            next,
            serde_json::to_value(&expected[generation + 1]).unwrap()
        );
    }
    assert!(
        simulation_store::get_step(&pool, id)
            .await
            .unwrap()
            .unwrap()
            .checkpoint
            .is_some()
    );
    pool.close().await;
}

#[tokio::test]
async fn stepping_has_no_cumulative_generation_or_history_budget_limit() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request: SimulationRequest = serde_json::from_value(life_request()).unwrap();
    request.generations = 999;
    request.initial_world.cells = vec![vec![request.initial_world.cells[0][0].clone(); 100]];
    let (history, checkpoint) = simulator::run_with_checkpoint(
        &request.automaton,
        &request.initial_world,
        999,
        request.seed,
        true,
    )
    .unwrap();
    let id = simulation_store::save_with_checkpoint(&pool, &request, &history, Some(&checkpoint))
        .await
        .unwrap();
    for generation in [999, 1000] {
        let (status, next) = call(
            &pool,
            "POST",
            &format!("/simulations/{id}/step"),
            Some(&json!({"generation":generation})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        assert_eq!(next["generation"], generation + 1);
    }
    assert_eq!(counts(&pool).await, (1, 1002));
    assert_eq!(
        simulation_store::get_step(&pool, id)
            .await
            .unwrap()
            .unwrap()
            .current
            .generation,
        1001
    );
    pool.close().await;
}

#[tokio::test]
async fn failed_steps_preserve_the_world_request_and_random_checkpoint() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request = life_request();
    request["generations"] = json!(0);
    request["automaton"]["rules"][0]["condition"] = json!({"kind":"always"});
    request["automaton"]["rules"][0]["probability"] = json!({"kind":"divide","args":[{"kind":"constant","args":1},{"kind":"count_neighbor_agents","args":{"type_id":1}}]});
    let (status, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let id = run["id"].as_i64().unwrap();
    let (_, before) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    let checkpoint: String =
        sqlx::query_scalar("SELECT state_json FROM simulation_checkpoints WHERE simulation_id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, error) = call(
        &pool,
        "POST",
        &format!("/simulations/{id}/step"),
        Some(&json!({"generation":0})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    let (_, after) = call(&pool, "GET", &format!("/simulations/{id}"), None).await;
    assert_eq!(before, after);
    let saved_checkpoint: String =
        sqlx::query_scalar("SELECT state_json FROM simulation_checkpoints WHERE simulation_id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint, saved_checkpoint);
    assert_eq!(counts(&pool).await, (1, 1));
    pool.close().await;
}

#[tokio::test]
async fn stale_steps_are_rejected_and_final_only_records_remain_final_only() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request = life_request();
    request["generations"] = json!(0);
    request["save_history"] = json!(false);
    let (_, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    let id = run["id"].as_i64().unwrap();
    let path = format!("/simulations/{id}/step");
    let (status, next) = call(&pool, "POST", &path, Some(&json!({"generation":0}))).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    let (status, _) = call(&pool, "POST", &path, Some(&json!({"generation":0}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(counts(&pool).await, (1, 1));
    let saved = simulation_store::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(saved.history[0].generation, 1);
    assert_eq!(saved.request.generations, 1);
    let (status, _) = call(
        &pool,
        "POST",
        "/simulations/99999/step",
        Some(&json!({"generation":0})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&pool, "POST", &path, Some(&json!({"generation":-1}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    pool.close().await;
}

#[tokio::test]
async fn storage_failure_rolls_back_the_entire_step() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let mut request = life_request();
    request["generations"] = json!(0);
    let (_, run) = call(&pool, "POST", "/simulations/run", Some(&request)).await;
    let id = run["id"].as_i64().unwrap();
    sqlx::raw_sql("CREATE TRIGGER reject_next BEFORE INSERT ON simulation_generations WHEN NEW.generation = 1 BEGIN SELECT RAISE(ABORT, 'test failure'); END;")
        .execute(&pool).await.unwrap();
    let (status, _) = call(
        &pool,
        "POST",
        &format!("/simulations/{id}/step"),
        Some(&json!({"generation":0})),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let saved = simulation_store::get(&pool, id).await.unwrap().unwrap();
    assert_eq!(saved.request.generations, 0);
    assert_eq!(counts(&pool).await, (1, 1));
    sqlx::raw_sql("DROP TRIGGER reject_next")
        .execute(&pool)
        .await
        .unwrap();
    let (status, next) = call(
        &pool,
        "POST",
        &format!("/simulations/{id}/step"),
        Some(&json!({"generation":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{next}");
    pool.close().await;
}
