mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use backend::{
    api,
    config::{db, state::AppState},
    models::simulation::{SimulationRequest, StoredSimulation},
    simulator::run,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const EXAMPLES: &[(&str, &str)] = &[
    (
        "game-of-life",
        include_str!("../examples/game-of-life.json"),
    ),
    ("forest-fire", include_str!("../examples/forest-fire.json")),
    ("diffusion", include_str!("../examples/diffusion.json")),
    (
        "artificial-ecosystem",
        include_str!("../examples/artificial-ecosystem.json"),
    ),
];

fn request(name: &str) -> SimulationRequest {
    serde_json::from_str(EXAMPLES.iter().find(|(key, _)| *key == name).unwrap().1).unwrap()
}

#[test]
fn individual_steps_with_restored_checkpoints_match_every_example_run() {
    for (name, _) in EXAMPLES {
        let request = request(name);
        let expected = frames(&request, request.generations);
        let (mut history, mut checkpoint) = backend::simulator::run_with_checkpoint(
            &request.automaton,
            &request.initial_world,
            0,
            request.seed,
            true,
        )
        .unwrap();
        let mut current = history.pop().unwrap();
        for expected in expected.iter().skip(1) {
            // Restore through JSON, just as when a new HTTP request reads SQLite.
            checkpoint =
                serde_json::from_str(&serde_json::to_string(&checkpoint).unwrap()).unwrap();
            (current, checkpoint) = backend::simulator::continue_step(
                &request.automaton,
                &current,
                request.seed,
                checkpoint,
            )
            .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                serde_json::to_value(&current).unwrap(),
                serde_json::to_value(expected).unwrap(),
                "{name}"
            );
        }
    }
}

fn frames(
    request: &SimulationRequest,
    steps: usize,
) -> Vec<backend::models::simulation::SimulationGeneration> {
    run(
        &request.automaton,
        &request.initial_world,
        steps,
        request.seed,
        true,
    )
    .unwrap()
}

#[test]
fn every_example_validates_replays_and_has_the_same_final_state_without_history() {
    for (name, _) in EXAMPLES {
        let request = request(name);
        request
            .validate_limits()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let history = frames(&request, request.generations);
        let replay = frames(&request, request.generations);
        let final_only = run(
            &request.automaton,
            &request.initial_world,
            request.generations,
            request.seed,
            false,
        )
        .unwrap();
        assert_eq!(history.len(), request.generations + 1, "{name}");
        assert_eq!(
            serde_json::to_value(&history).unwrap(),
            serde_json::to_value(replay).unwrap(),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(history.last().unwrap()).unwrap(),
            serde_json::to_value(&final_only[0]).unwrap(),
            "{name}"
        );
        for frame in &history {
            request.automaton.validate_world(&frame.world).unwrap();
        }
    }
}

#[tokio::test]
async fn every_example_runs_through_the_api_and_round_trips_configuration_and_history_in_sqlite() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = api::router().with_state(AppState { db: pool.clone() });
    for (name, source) in EXAMPLES {
        let example = app
            .clone()
            .oneshot(
                Request::get(format!("/simulations/examples/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(example.status(), StatusCode::OK);
        let body = to_bytes(example.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::to_value(request(name)).unwrap()
        );
        let validation = app
            .clone()
            .oneshot(
                Request::post("/simulations/validate")
                    .header("content-type", "application/json")
                    .body(Body::from(*source))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = validation.status();
        let body = to_bytes(validation.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{name}: {}",
            String::from_utf8_lossy(&body)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"valid":true})
        );
        let response = app
            .clone()
            .oneshot(
                Request::post("/simulations/run")
                    .header("content-type", "application/json")
                    .body(Body::from(*source))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{name}: {}",
            String::from_utf8_lossy(&body)
        );
        let result: Value = serde_json::from_slice(&body).unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/simulations/{}", result["id"]))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{name}");
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        let stored: StoredSimulation = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            serde_json::to_value(&stored.request).unwrap(),
            serde_json::to_value(request(name)).unwrap(),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(stored.history).unwrap(),
            result["history"],
            "{name}"
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn unknown_example_returns_a_json_not_found_error() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = api::router().with_state(AppState { db: pool.clone() });
    for name in ["missing", "traffic"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/simulations/examples/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":"Example not found"})
        );
    }
    pool.close().await;
}

#[test]
fn forest_fire_burns_old_fires_out_and_ignites_adjacent_trees() {
    let history = frames(&request("forest-fire"), 2);
    assert_eq!(history[1].world.cells[2][2].type_id, 0);
    for (row, col) in [
        (1, 1),
        (1, 2),
        (1, 3),
        (2, 1),
        (2, 3),
        (3, 1),
        (3, 2),
        (3, 3),
    ] {
        assert_eq!(history[1].world.cells[row][col].type_id, 2);
        assert_eq!(history[2].world.cells[row][col].type_id, 0);
    }
}

#[test]
fn global_relocation_can_reach_a_free_cell_outside_the_local_neighborhood() {
    let mut value = common::free_movement();
    value["automaton"]["rules"][0]["actions"] =
        json!([{"kind":"move_to_free_cell","selection":"first","cell_type":0}]);
    value["initial_world"]["agents"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    value["initial_world"]["agents"][0]["position"]["col"] = json!(5);
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    let history = frames(&request, 1);
    assert_eq!(history[1].world.agents[0].position.col, 0);
}

#[test]
fn global_relocation_respects_type_old_occupancy_and_competing_destinations() {
    let mut value = common::free_movement();
    value["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":2,"name":"park","properties":{}}));
    value["automaton"]["rules"][0]["actions"] =
        json!([{"kind":"move_to_free_cell","selection":"first","cell_type":2}]);
    value["initial_world"]["cells"][0][0]["type_id"] = json!(2);
    let request: SimulationRequest = serde_json::from_value(value.clone()).unwrap();
    let history = frames(&request, 1);
    assert_eq!(
        serde_json::to_value(&history[1].world.agents).unwrap(),
        value["initial_world"]["agents"]
    );
    value["initial_world"]["agents"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    let history = frames(&request, 2);
    assert_eq!(history[1].world.agents[0].position.col, 0);
    assert_eq!(history[2].world.agents[0].position.col, 0);
}

#[test]
fn a_requested_wrapped_alias_still_reaches_a_free_neighbor() {
    let mut value = common::free_movement();
    value["automaton"]["neighborhood"]["offsets"] = json!([[0, -1], [0, 1]]);
    value["initial_world"]["cells"][0]
        .as_array_mut()
        .unwrap()
        .truncate(2);
    value["initial_world"]["agents"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    value["initial_world"]["agents"][0]["position"]["col"] = json!(0);
    value["automaton"]["rules"][0]["actions"][0]["offsets"] = json!([[0, 1]]);
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    assert_eq!(frames(&request, 1)[1].world.agents[0].position.col, 1);
}

#[test]
fn an_ecosystem_fox_moves_to_the_selected_rabbit_and_gets_its_configured_reward() {
    let request = request("artificial-ecosystem");
    let history = frames(&request, 1);
    let world = &history[1].world;
    assert_eq!(
        world
            .agents
            .iter()
            .filter(|agent| agent.state.type_id == 2)
            .count(),
        1
    );
    let victim = request
        .initial_world
        .agents
        .iter()
        .find(|old| !world.agents.iter().any(|new| new.id == old.id))
        .unwrap();
    let fox = world
        .agents
        .iter()
        .find(|agent| agent.state.type_id == 3)
        .unwrap();
    assert_eq!(fox.position, victim.position);
    assert_eq!(fox.state.properties["energy"], 17.);
    assert_eq!(
        world.cells[victim.position.row][victim.position.col].type_id,
        1
    );
}

#[test]
fn ecosystem_reproduction_creates_a_new_individual_with_configured_energy() {
    let mut request = request("artificial-ecosystem");
    request.initial_world.agents.truncate(1);
    request.initial_world.agents[0]
        .state
        .properties
        .insert("energy".to_owned(), 16.);
    let history = frames(&request, 1);
    let agents = &history[1].world.agents;
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].state.properties["energy"], 12.);
    assert_eq!(agents[1].state.properties["energy"], 4.);
    assert_ne!(agents[0].id, agents[1].id);
}

#[test]
fn new_movement_options_reject_invalid_types_offsets_and_combinations_before_execution() {
    let base = common::free_movement();
    for actions in [
        json!([{"kind":"move_to_free_cell","selection":"first","cell_type":99}]),
        json!([{"kind":"move_to_free_neighbor","selection":"first","offsets":[[0,-1]]}]),
        json!([{"kind":"move_to_free_neighbor","selection":"first","offsets":[[0,1],[0,1]]}]),
        json!([{"kind":"move_to_removed_neighbor"}]),
        json!([{"kind":"move_to_free_cell","selection":"first"},{"kind":"move_to_free_neighbor","selection":"first"}]),
        json!([{"kind":"remove_agent"},{"kind":"move_to_free_cell","selection":"first"}]),
    ] {
        let mut value = base.clone();
        value["automaton"]["rules"][0]["actions"] = actions;
        value["automaton"]["rules"][0]["probability"] = json!(0);
        let request: SimulationRequest = serde_json::from_value(value).unwrap();
        assert!(run(&request.automaton, &request.initial_world, 0, 42, true).is_err());
    }
    for action in [
        json!({"kind":"move_to_free_cell","selection":"first"}),
        json!({"kind":"move_to_removed_neighbor"}),
    ] {
        let mut value = base.clone();
        value["automaton"]["rules"][0]["target"] = json!("cell");
        value["automaton"]["rules"][0]["actions"] = json!([action]);
        let request: SimulationRequest = serde_json::from_value(value).unwrap();
        assert!(
            run(&request.automaton, &request.initial_world, 0, 42, true)
                .unwrap_err()
                .contains("only allowed in agent rules")
        );
    }
}
