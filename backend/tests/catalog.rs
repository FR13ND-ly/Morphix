use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use backend::{
    api,
    config::{db, state::AppState},
    models::{
        position::Position,
        simulation::{SimulationRequest, StoredSimulation},
    },
    simulator::run,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const EXAMPLES: &[(&str, &str)] = &[
    (
        "game-of-life",
        include_str!("../examples/game-of-life.json"),
    ),
    (
        "moving-agents",
        include_str!("../examples/moving-agents.json"),
    ),
    ("rule-30", include_str!("../examples/rule-30.json")),
    (
        "predator-prey",
        include_str!("../examples/predator-prey.json"),
    ),
    ("diffusion", include_str!("../examples/diffusion.json")),
    (
        "brians-brain",
        include_str!("../examples/brians-brain.json"),
    ),
    ("cyclic", include_str!("../examples/cyclic.json")),
    (
        "langtons-ant",
        include_str!("../examples/langtons-ant.json"),
    ),
    ("forest-fire", include_str!("../examples/forest-fire.json")),
    ("sandpile", include_str!("../examples/sandpile.json")),
    ("traffic", include_str!("../examples/traffic.json")),
    ("epidemic", include_str!("../examples/epidemic.json")),
    ("schelling", include_str!("../examples/schelling.json")),
    ("voter", include_str!("../examples/voter.json")),
    (
        "crystal-growth",
        include_str!("../examples/crystal-growth.json"),
    ),
    (
        "falling-sand",
        include_str!("../examples/falling-sand.json"),
    ),
    (
        "artificial-ecosystem",
        include_str!("../examples/artificial-ecosystem.json"),
    ),
];

fn request(name: &str) -> SimulationRequest {
    serde_json::from_str(EXAMPLES.iter().find(|(key, _)| *key == name).unwrap().1).unwrap()
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
    let response = app
        .oneshot(
            Request::get("/simulations/examples/missing")
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
    pool.close().await;
}

#[test]
fn brians_brain_has_firing_dying_off_transitions_and_birth_from_two_firing_neighbors() {
    let history = frames(&request("brians-brain"), 2);
    assert_eq!(history[1].world.cells[2][1].type_id, 2);
    assert_eq!(history[2].world.cells[2][1].type_id, 0);
    assert_eq!(history[1].world.cells[2][2].type_id, 1);
    assert_eq!(history[2].world.cells[2][2].type_id, 2);
}

#[test]
fn cyclic_changes_only_when_at_least_two_neighbors_have_the_next_phase() {
    let request = request("cyclic");
    let history = frames(&request, 1);
    let cells = &request.initial_world.cells;
    for row in 0..4 {
        for col in 0..4 {
            let old = cells[row][col].type_id;
            let next = (old + 1) % 4;
            let count = (-1i32..=1)
                .flat_map(|dr| (-1i32..=1).map(move |dc| (dr, dc)))
                .filter(|offset| *offset != (0, 0))
                .filter(|(dr, dc)| {
                    cells[(row as i32 + dr).rem_euclid(4) as usize]
                        [(col as i32 + dc).rem_euclid(4) as usize]
                        .type_id
                        == next
                })
                .count();
            assert_eq!(
                history[1].world.cells[row][col].type_id,
                if count >= 2 { next } else { old }
            );
        }
    }
}

#[test]
fn langtons_ant_turns_flips_the_old_cell_and_moves_using_the_new_direction() {
    let history = frames(&request("langtons-ant"), 5);
    for (frame, (row, col, direction)) in
        history
            .iter()
            .skip(1)
            .zip([(3, 4, 1.), (4, 4, 2.), (4, 3, 3.), (3, 3, 0.), (3, 2, 3.)])
    {
        assert_eq!(frame.world.agents[0].position, Position { row, col });
        assert_eq!(
            frame.world.agents[0].state.properties["direction"],
            direction
        );
    }
    assert_eq!(history[1].world.cells[3][3].type_id, 1);
    assert_eq!(history[4].world.cells[3][3].type_id, 1);
    assert_eq!(history[5].world.cells[3][3].type_id, 0);
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
fn traffic_moves_only_into_positions_that_were_free_at_the_start_of_the_step() {
    let history = frames(&request("traffic"), 1);
    assert_eq!(
        history[1]
            .world
            .agents
            .iter()
            .map(|agent| agent.position.col)
            .collect::<Vec<_>>(),
        vec![2, 3, 5, 9]
    );
}

#[test]
fn infection_has_a_timer_and_recovered_cells_remain_recovered() {
    let history = frames(&request("epidemic"), 8);
    assert_eq!(history[1].world.cells[1][1].properties["days"], 1.);
    assert_eq!(history[2].world.cells[1][1].properties["days"], 2.);
    assert_eq!(history[3].world.cells[1][1].type_id, 2);
    for pair in history.windows(2) {
        for (old, new) in pair[0]
            .world
            .cells
            .iter()
            .flatten()
            .zip(pair[1].world.cells.iter().flatten())
        {
            if old.type_id == 2 {
                assert_eq!(new.type_id, 2);
            }
        }
    }
}

#[test]
fn global_relocation_can_reach_a_free_cell_outside_the_local_neighborhood() {
    let mut value = serde_json::to_value(request("traffic")).unwrap();
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
    let mut value = serde_json::to_value(request("traffic")).unwrap();
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
fn an_isolated_schelling_agent_does_not_divide_by_zero_or_relocate() {
    let mut request = request("schelling");
    request.initial_world.agents.truncate(1);
    let history = frames(&request, 2);
    assert_eq!(
        history[2].world.agents[0].position,
        request.initial_world.agents[0].position
    );
}

#[test]
fn voter_adopts_a_strict_majority_and_preserves_its_opinion_on_a_tie() {
    let mut request = request("voter");
    assert_eq!(frames(&request, 1)[1].world.cells[1][1].type_id, 0);
    request.initial_world.cells[0][0].type_id = 1;
    assert_eq!(frames(&request, 1)[1].world.cells[1][1].type_id, 1);
}

#[test]
fn one_crystal_seed_grows_a_ring_without_erasing_the_seed() {
    let history = frames(&request("crystal-growth"), 1);
    assert_eq!(
        history[1]
            .world
            .cells
            .iter()
            .flatten()
            .filter(|cell| cell.type_id == 1)
            .count(),
        9
    );
    assert_eq!(history[1].world.cells[2][2].type_id, 1);
}

#[test]
fn particles_follow_their_own_direction_priority_and_preserve_particle_count() {
    let history = frames(&request("falling-sand"), 8);
    assert_eq!(
        history[1].world.agents[0].position,
        Position { row: 1, col: 1 }
    );
    assert_eq!(
        history[1].world.agents[1].position,
        Position { row: 1, col: 4 }
    );
    for frame in history {
        assert_eq!(frame.world.agents.len(), 2);
        for agent in frame.world.agents {
            assert_eq!(
                frame.world.cells[agent.position.row][agent.position.col].type_id,
                0
            );
        }
    }
}

#[test]
fn a_requested_wrapped_alias_still_reaches_a_free_neighbor() {
    let mut value = serde_json::to_value(request("traffic")).unwrap();
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
    let base = serde_json::to_value(request("traffic")).unwrap();
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
