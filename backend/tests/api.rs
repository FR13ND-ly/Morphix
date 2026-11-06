mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use backend::{
    api,
    config::{db, state::AppState},
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn life_request() -> Value {
    serde_json::from_str(include_str!("../examples/game-of-life.json")).unwrap()
}

fn numeric_request() -> Value {
    json!({
        "automaton": {
            "name": "Counter",
            "object_types": [{
                "id": 0,
                "name": "field",
                "properties": {
                    "value": {
                        "default_value": 0.0,
                        "min_value": 0.0,
                        "max_value": 99.0,
                        "integer_only": false
                    }
                }
            }],
            "neighborhood": { "offsets": [[0, 1]] },
            "boundary": { "kind": "wrap" },
            "rules": [{
                "target": "cell",
                "condition": { "kind": "always" },
                "probability": 1.0,
                "actions": [{
                    "kind": "set_property",
                    "property": "value",
                    "value": {
                        "kind": "add",
                        "args": [
                            { "kind": "property", "args": "value" },
                            { "kind": "constant", "args": 1.0 }
                        ]
                    }
                }]
            }]
        },
        "initial_world": {
            "cells": [[{ "type_id": 0, "properties": { "value": 98.0 } }]],
            "agents": []
        },
        "generations": 1,
        "seed": 42
    })
}

async fn send(body: String, content_type: &str) -> (StatusCode, Value) {
    send_to("/simulations/run", body, content_type).await
}

async fn send_to(path: &str, body: String, content_type: &str) -> (StatusCode, Value) {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let response = api::router()
        .with_state(AppState { db: pool.clone() })
        .oneshot(
            Request::post(path)
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(response.headers()["content-type"], "application/json");
    let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    pool.close().await;
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn post(request: &Value) -> (StatusCode, Value) {
    send(request.to_string(), "application/json").await
}

async fn validate(request: &Value) -> (StatusCode, Value) {
    send_to(
        "/simulations/validate",
        request.to_string(),
        "application/json",
    )
    .await
}

#[tokio::test]
async fn validates_a_simulation_without_returning_a_run_or_generation() {
    for save_history in [true, false] {
        let mut request = life_request();
        request["save_history"] = json!(save_history);
        let (status, response) = validate(&request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, json!({"valid": true}));
    }
}

#[tokio::test]
async fn validation_and_execution_report_the_same_static_errors_and_limits() {
    let mut unknown_rule_type = life_request();
    unknown_rule_type["generations"] = json!(0);
    unknown_rule_type["automaton"]["rules"][0]["probability"] = json!(0);
    unknown_rule_type["automaton"]["rules"][0]["actions"][0]["type_id"] = json!(99);

    let mut unknown_cell_type = life_request();
    unknown_cell_type["initial_world"]["cells"][0][0]["type_id"] = json!(99);

    let mut ragged = life_request();
    ragged["initial_world"]["cells"][0]
        .as_array_mut()
        .unwrap()
        .pop();

    let mut invalid_initial_value = numeric_request();
    invalid_initial_value["initial_world"]["cells"][0][0]["properties"]["value"] = json!(100);

    let mut too_many_steps = life_request();
    too_many_steps["generations"] = json!(1001);

    let mut too_much_history = life_request();
    too_much_history["generations"] = json!(1000);
    too_much_history["initial_world"]["cells"] =
        json!([vec![json!({"type_id":0,"properties":{}}); 100]]);

    let mut too_deep = numeric_request();
    let mut expression = json!({"kind":"property","args":"value"});
    for _ in 0..18 {
        expression = json!({"kind":"add","args":[expression,{"kind":"constant","args":0}]});
    }
    too_deep["automaton"]["rules"][0]["actions"][0]["value"] = expression;

    for request in [
        unknown_rule_type,
        unknown_cell_type,
        ragged,
        invalid_initial_value,
        too_many_steps,
        too_much_history,
        too_deep,
    ] {
        let (status, response) = validate(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(
            response["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty())
        );
        let (run_status, run_response) = post(&request).await;
        assert_eq!(status, run_status);
        assert_eq!(response, run_response);
    }
}

#[tokio::test]
async fn static_validation_does_not_evaluate_rules_or_promise_runtime_success() {
    let mut division_by_zero = numeric_request();
    division_by_zero["automaton"]["rules"][0]["actions"][0]["value"] = json!({
        "kind":"divide", "args":[{"kind":"constant","args":1}, {
            "kind":"subtract", "args":[{"kind":"property","args":"value"},{"kind":"constant","args":98}]
        }]
    });
    let mut invalid_dynamic_probability = numeric_request();
    invalid_dynamic_probability["automaton"]["rules"][0]["probability"] =
        json!({"kind":"property","args":"value"});
    for request in [division_by_zero, invalid_dynamic_probability] {
        let (status, response) = validate(&request).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert_eq!(response, json!({"valid":true}));
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].as_str().unwrap().contains("Generation 1"));
    }
}

#[tokio::test]
async fn validation_does_not_write_to_or_require_a_live_database_connection() {
    let pool = db::connect("sqlite::memory:").await.unwrap();
    let app = api::router().with_state(AppState { db: pool.clone() });
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::post("/simulations/validate")
                    .header("content-type", "application/json")
                    .body(Body::from(life_request().to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    for table in ["simulations", "simulation_generations"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    pool.close().await;
    let response = app
        .oneshot(
            Request::post("/simulations/validate")
                .header("content-type", "application/json")
                .body(Body::from(life_request().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn validation_keeps_json_schema_content_type_and_body_limit_errors() {
    let mut unknown_field = life_request();
    unknown_field["unexpected"] = json!(true);
    let mut too_large = life_request();
    too_large["automaton"]["name"] = json!("x".repeat(256 * 1024));
    for (body, content_type, expected) in [
        ("{".to_owned(), "application/json", StatusCode::BAD_REQUEST),
        (
            life_request().to_string(),
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            unknown_field.to_string(),
            "application/json",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            too_large.to_string(),
            "application/json",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let (status, response) = send_to("/simulations/validate", body, content_type).await;
        assert_eq!(status, expected, "{response}");
        assert!(response["error"].is_string());
        assert!(response.get("valid").is_none());
    }
}

fn types(world: &Value) -> Vec<Vec<u64>> {
    world["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|cell| cell["type_id"].as_u64().unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn runs_the_game_of_life_example_through_the_http_router() {
    let request = life_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["seed"], 42);
    assert!(response["id"].as_i64().unwrap() > 0);
    assert_eq!(response["history"].as_array().unwrap().len(), 3);
    assert_eq!(response["history"][0]["world"], request["initial_world"]);
    assert_eq!(
        types(&response["history"][1]["world"]),
        vec![vec![0, 1, 0]; 3]
    );
    assert_eq!(response["history"][2]["world"], request["initial_world"]);
    for (generation, snapshot) in response["history"].as_array().unwrap().iter().enumerate() {
        assert_eq!(snapshot["generation"], generation);
    }
}

#[tokio::test]
async fn accepts_zero_steps_and_returns_only_the_validated_initial_world() {
    let mut request = life_request();
    request["generations"] = json!(0);

    for save_history in [true, false] {
        request["save_history"] = json!(save_history);
        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response["history"],
            json!([{ "generation": 0, "world": request["initial_world"] }])
        );
    }
}

#[tokio::test]
async fn returns_only_the_numbered_final_generation_when_history_is_disabled() {
    let mut request = life_request();
    request["generations"] = json!(3);
    request["save_history"] = json!(true);
    let (full_status, full) = post(&request).await;
    assert_eq!(full_status, StatusCode::OK);
    assert_eq!(full["history"].as_array().unwrap().len(), 4);

    request["save_history"] = json!(false);
    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["seed"], request["seed"]);
    assert_eq!(response["history"].as_array().unwrap().len(), 1);
    assert_eq!(response["history"][0]["generation"], 3);
    assert_eq!(
        types(&response["history"][0]["world"]),
        vec![vec![0, 1, 0]; 3]
    );
    assert_eq!(response["history"][0], full["history"][3]);
}

#[tokio::test]
async fn rejects_non_boolean_history_options() {
    for invalid in [json!(null), json!(0), json!("false"), json!({})] {
        let mut request = life_request();
        request["save_history"] = invalid;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].as_str().unwrap().contains("save_history"));
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn final_only_requests_still_limit_generation_count_and_initial_world_size() {
    let mut too_many_steps = life_request();
    too_many_steps["save_history"] = json!(false);
    too_many_steps["generations"] = json!(1001);

    let mut too_large_world = numeric_request();
    too_large_world["save_history"] = json!(false);
    too_large_world["generations"] = json!(0);
    too_large_world["initial_world"]["cells"] = json!([vec![
        too_large_world["initial_world"]["cells"][0][0].clone(); 1_000
    ]]);
    too_large_world["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 1,
            "name": "large",
            "properties": (0..100).map(|index| (format!("property_{index}"), json!({
                "default_value": 0.0,
                "min_value": 0.0,
                "max_value": 1.0,
                "integer_only": false
            }))).collect::<serde_json::Map<String, Value>>()
        }));

    for (request, message) in [
        (too_many_steps, "generations"),
        (too_large_world, "history"),
    ] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].as_str().unwrap().contains(message));
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn final_only_requests_report_intermediate_generation_errors_without_partial_results() {
    let mut request = numeric_request();
    request["generations"] = json!(3);
    request["save_history"] = json!(false);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    let error = response["error"].as_str().unwrap();
    assert!(error.contains("Generation 2"), "{error}");
    assert!(error.contains("value"), "{error}");
    assert!(response.get("history").is_none());
}

#[tokio::test]
async fn reports_json_syntax_content_type_and_schema_errors_as_json() {
    let mut unknown_field = life_request();
    unknown_field["generatons"] = json!(2);
    let mut unknown_nested_field = life_request();
    unknown_nested_field["initial_world"]["cells"][0][0]["typo"] = json!(0);
    let mut invalid_id = life_request();
    invalid_id["initial_world"]["cells"][0][0]["type_id"] = json!(-1);
    let mut invalid_variant = life_request();
    invalid_variant["automaton"]["boundary"]["kind"] = json!("invalid");

    for (body, content_type, expected) in [
        ("{".to_owned(), "application/json", StatusCode::BAD_REQUEST),
        (
            life_request().to_string(),
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            unknown_field.to_string(),
            "application/json",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            unknown_nested_field.to_string(),
            "application/json",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            invalid_id.to_string(),
            "application/json",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            invalid_variant.to_string(),
            "application/json",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let (status, response) = send(body, content_type).await;
        assert_eq!(status, expected);
        assert!(!response["error"].as_str().unwrap().is_empty());
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn rejects_request_bodies_above_the_http_size_limit() {
    let body = format!("{}{}", " ".repeat(256 * 1024), life_request());

    let (status, response) = send(body, "application/json").await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!response["error"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn returns_model_validation_errors_including_for_zero_step_requests() {
    for generations in [0, 2] {
        let mut unknown_type = life_request();
        unknown_type["generations"] = json!(generations);
        unknown_type["initial_world"]["cells"][0][0]["type_id"] = json!(99);
        let mut invalid_probability = life_request();
        invalid_probability["generations"] = json!(generations);
        invalid_probability["automaton"]["rules"][0]["probability"] = json!(-0.1);

        for (request, expected_message) in [
            (unknown_type, "Unknown object type id 99"),
            (invalid_probability, "probability"),
        ] {
            let (status, response) = post(&request).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            let error = response["error"].as_str().unwrap();
            assert!(error.contains("Generation 0"));
            assert!(error.contains(expected_message));
        }
    }
}

#[tokio::test]
async fn identifies_the_generation_and_cell_where_execution_fails() {
    let mut request = numeric_request();
    request["generations"] = json!(2);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let error = response["error"].as_str().unwrap();
    assert!(error.contains("Generation 2"));
    assert!(error.contains("Cell (0, 0)"));
    assert!(error.contains("Rule 0"));
    assert!(error.contains("value"));
    assert!(response.get("history").is_none());
}

#[tokio::test]
async fn updates_continuous_cell_and_agent_properties_from_json() {
    let mut request = numeric_request();
    request["initial_world"]["cells"][0][0]["properties"]["value"] = json!(2.5);
    request["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 1,
            "name": "rabbit",
            "properties": {
                "energy": {
                    "default_value": 10.0,
                    "min_value": 0.0,
                    "max_value": 100.0,
                    "integer_only": false
                }
            }
        }));
    request["initial_world"]["agents"] = json!([{
        "id": 7,
        "position": { "row": 0, "col": 0 },
        "state": { "type_id": 1, "properties": { "energy": 10.0 } }
    }]);
    request["automaton"]["rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "target": "agent",
            "condition": { "kind": "always" },
            "probability": 1.0,
            "actions": [{
                "kind": "set_property",
                "property": "energy",
                "value": {
                    "kind": "add",
                    "args": [
                        { "kind": "property", "args": "energy" },
                        { "kind": "sum_neighbors", "args": { "property": "value" } }
                    ]
                }
            }]
        }));

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK);
    let next = &response["history"][1]["world"];
    assert_eq!(next["cells"][0][0]["properties"]["value"], 3.5);
    assert_eq!(next["agents"][0]["state"]["properties"]["energy"], 12.5);
    assert_eq!(next["agents"][0]["id"], 7);
    assert_eq!(next["agents"][0]["position"], json!({ "row": 0, "col": 0 }));
}

#[tokio::test]
async fn reproduces_probabilistic_http_runs_with_the_same_seed() {
    let mut request = numeric_request();
    request["generations"] = json!(32);
    request["initial_world"]["cells"][0][0]["properties"]["value"] = json!(0.0);
    request["automaton"]["rules"][0]["probability"] = json!(0.5);

    let (status, first) = post(&request).await;
    let (replay_status, replay) = post(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(first["seed"], replay["seed"]);
    assert_eq!(first["history"], replay["history"]);
    assert_eq!(first["history"].as_array().unwrap().len(), 33);
}

#[tokio::test]
async fn rejects_excessive_steps_history_and_definition_sizes() {
    let mut steps = life_request();
    steps["generations"] = json!(1001);
    let mut history = life_request();
    history["generations"] = json!(1000);
    history["initial_world"]["cells"] =
        json!([vec![json!({ "type_id": 0, "properties": {} }); 100]]);
    let mut rules = life_request();
    rules["automaton"]["rules"] = json!(vec![rules["automaton"]["rules"][0].clone(); 65]);
    let mut offsets = life_request();
    offsets["automaton"]["neighborhood"]["offsets"] =
        json!((1..=33).map(|i| vec![0, i]).collect::<Vec<_>>());
    let mut nodes = life_request();
    nodes["automaton"]["rules"][0]["condition"] = json!({
        "kind": "all", "args": vec![json!({ "kind": "always" }); 513]
    });
    let mut depth = life_request();
    let mut condition = json!({ "kind": "always" });
    for _ in 0..17 {
        condition = json!({ "kind": "not", "args": condition });
    }
    depth["automaton"]["rules"][0]["condition"] = condition;

    for (request, expected_message) in [
        (steps, "generations"),
        (history, "history"),
        (rules, "rules"),
        (offsets, "offsets"),
        (nodes, "nodes"),
        (depth, "depth"),
    ] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"]
                .as_str()
                .unwrap()
                .contains(expected_message)
        );
    }
}

#[tokio::test]
async fn accounts_for_properties_of_types_that_actions_can_create() {
    let mut request = life_request();
    request["generations"] = json!(1000);
    request["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 2,
            "name": "many_properties",
            "properties": (0..16).map(|i| (format!("property_{i}"), json!({
                "default_value": 0.0,
                "min_value": 0.0,
                "max_value": 1.0,
                "integer_only": false
            }))).collect::<serde_json::Map<String, Value>>()
        }));
    request["automaton"]["rules"][0]["actions"][0]["type_id"] = json!(2);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(response["error"].as_str().unwrap().contains("history"));
}

#[tokio::test]
async fn rejects_deep_action_expressions_and_long_property_names() {
    let mut deep_expression = numeric_request();
    let mut expression = json!({ "kind": "constant", "args": 0.0 });
    for _ in 0..17 {
        expression = json!({
            "kind": "add",
            "args": [expression, { "kind": "constant", "args": 0.0 }]
        });
    }
    deep_expression["automaton"]["rules"][0]["actions"][0]["value"] = expression;

    let mut long_name = numeric_request();
    let name = "x".repeat(65);
    let definition = long_name["automaton"]["object_types"][0]["properties"]["value"].clone();
    long_name["automaton"]["object_types"][0]["properties"] = json!({ name.clone(): definition });
    long_name["initial_world"]["cells"][0][0]["properties"] = json!({ name.clone(): 98.0 });
    long_name["automaton"]["rules"][0]["actions"][0]["property"] = json!(name.clone());
    long_name["automaton"]["rules"][0]["actions"][0]["value"]["args"][0]["args"] = json!(name);

    for (request, message) in [(deep_expression, "depth"), (long_name, "Property names")] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(response["error"].as_str().unwrap().contains(message));
    }
}

#[tokio::test]
async fn rejects_invalid_unused_rules_before_even_a_zero_step_run() {
    let base_rule = json!({
        "target": "agent",
        "condition": { "kind": "always" },
        "probability": 0.0,
        "actions": [{ "kind": "set_type", "type_id": 1 }]
    });
    let mut unknown_type = base_rule.clone();
    unknown_type["condition"] = json!({ "kind": "any", "args": [
        { "kind": "always" }, { "kind": "type_is", "args": 99 }
    ] });
    let mut unknown_action_type = base_rule.clone();
    unknown_action_type["actions"][0]["type_id"] = json!(99);
    let mut missing_property = base_rule.clone();
    missing_property["condition"] = json!({ "kind": "compare", "args": {
        "left": { "kind": "property", "args": "missing" },
        "operator": { "kind": "equal" },
        "right": { "kind": "constant", "args": 0 }
    }});
    let mut bad_tolerance = base_rule;
    bad_tolerance["condition"] = json!({ "kind": "compare", "args": {
        "left": { "kind": "constant", "args": 0 },
        "operator": { "kind": "approximately_equal", "tolerance": -1.0 },
        "right": { "kind": "constant", "args": 0 }
    }});

    for generations in [0, 2] {
        for (rule, message) in [
            (&unknown_type, "Unknown object type id 99"),
            (&unknown_action_type, "Action 0: Unknown object type id 99"),
            (&missing_property, "Unknown property 'missing'"),
            (&bad_tolerance, "tolerance"),
        ] {
            let mut request = life_request();
            request["generations"] = json!(generations);
            request["automaton"]["rules"]
                .as_array_mut()
                .unwrap()
                .push(rule.clone());

            let (status, response) = post(&request).await;

            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            let error = response["error"].as_str().unwrap();
            assert!(error.contains("Generation 0: Rule 2:"), "{error}");
            assert!(error.contains(message), "{error}");
            assert!(response.get("history").is_none());
        }
    }
}

#[tokio::test]
async fn rejects_invalid_constant_arithmetic_before_the_first_generation() {
    for (expression, message) in [
        (
            json!({ "kind": "divide", "args": [
            { "kind": "property", "args": "value" },
            { "kind": "subtract", "args": [
                { "kind": "constant", "args": 2 },
                { "kind": "constant", "args": 2 }
            ] }
        ] }),
            "Division by zero",
        ),
        (
            json!({ "kind": "add", "args": [
            { "kind": "constant", "args": 1e308 },
            { "kind": "constant", "args": 1e308 }
        ] }),
            "finite",
        ),
    ] {
        let mut request = numeric_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][0]["actions"][0]["value"] = expression;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 0: Action 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

fn movement_request() -> Value {
    serde_json::from_value(common::moving_agents()).unwrap()
}

#[tokio::test]
async fn moves_agents_and_resolves_conflicts_through_the_http_endpoint() {
    let request = movement_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["history"].as_array().unwrap().len(), 4);
    assert_eq!(
        response["history"][0]["world"]["cells"],
        request["initial_world"]["cells"]
    );
    for index in 0..2 {
        let initial = &response["history"][0]["world"]["agents"][index];
        let expected = &request["initial_world"]["agents"][index];
        assert_eq!(initial["id"], expected["id"]);
        assert_eq!(initial["position"], expected["position"]);
        assert_eq!(initial["state"]["type_id"], expected["state"]["type_id"]);
        assert_eq!(
            initial["state"]["properties"]["step"].as_f64(),
            expected["state"]["properties"]["step"].as_f64()
        );
    }
    for generation in 1..=3 {
        let world = &response["history"][generation]["world"];
        assert_eq!(
            world["agents"][0]["position"],
            json!({ "row": 0, "col": 1 })
        );
        assert_eq!(
            world["agents"][1]["position"],
            json!({ "row": 0, "col": 3 })
        );
        assert_eq!(world["cells"], request["initial_world"]["cells"]);
        assert_eq!(world["agents"][0]["id"], 7);
        assert_eq!(world["agents"][1]["id"], 8);
    }
}

#[tokio::test]
async fn rejects_invalid_movement_definitions_including_at_zero_steps() {
    let mut cell_rule = movement_request();
    cell_rule["automaton"]["rules"][0]["target"] = json!("cell");
    let mut fractional = movement_request();
    fractional["automaton"]["rules"][0]["actions"][0]["col_offset"] =
        json!({ "kind": "constant", "args": 0.5 });
    let mut duplicate = movement_request();
    let action = duplicate["automaton"]["rules"][0]["actions"][0].clone();
    duplicate["automaton"]["rules"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(action);
    let mut unknown_property = movement_request();
    unknown_property["automaton"]["rules"][0]["actions"][0]["row_offset"] =
        json!({ "kind": "property", "args": "missing" });

    for (mut request, message) in [
        (cell_rule, "Move is only allowed in agent rules"),
        (fractional, "Movement offset must be a finite integer"),
        (duplicate, "at most one movement action"),
        (unknown_property, "Unknown property 'missing'"),
    ] {
        request["generations"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 0: Action"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn applies_expression_limits_to_both_movement_offsets() {
    for axis in ["row_offset", "col_offset"] {
        let mut request = movement_request();
        let mut expression = json!({ "kind": "constant", "args": 0 });
        for _ in 0..17 {
            expression =
                json!({ "kind": "add", "args": [expression, { "kind": "constant", "args": 0 }] });
        }
        request["automaton"]["rules"][0]["actions"][0][axis] = expression;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(response["error"].as_str().unwrap().contains("depth"));
    }
}

#[tokio::test]
async fn accepts_numeric_and_expression_probabilities_in_existing_requests() {
    for (probability, expected_value) in [
        (json!(0.0), 98.0),
        (json!(1.0), 99.0),
        (json!({ "kind": "constant", "args": 0.0 }), 98.0),
        (json!({ "kind": "constant", "args": 1.0 }), 99.0),
        (
            json!({ "kind": "divide", "args": [
            { "kind": "property", "args": "value" },
            { "kind": "constant", "args": 98.0 }
        ] }),
            99.0,
        ),
    ] {
        let mut request = numeric_request();
        request["automaton"]["rules"][0]["probability"] = probability;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::OK, "{response}");
        assert_eq!(
            response["history"][1]["world"]["cells"][0][0]["properties"]["value"].as_f64(),
            Some(expected_value)
        );
    }
}

#[tokio::test]
async fn uses_a_neighbor_based_probability_formula_and_reproduces_the_result() {
    let mut request = life_request();
    request["generations"] = json!(1);
    request["initial_world"]["cells"] =
        json!(vec![vec![json!({ "type_id": 0, "properties": {} }); 3]; 3]);
    request["initial_world"]["cells"][0][1]["type_id"] = json!(1);
    let probability = json!({ "kind": "subtract", "args": [
        { "kind": "constant", "args": 1.0 },
        { "kind": "power", "args": [
            { "kind": "subtract", "args": [
                { "kind": "constant", "args": 1.0 },
                { "kind": "constant", "args": 1.0 }
            ] },
            { "kind": "count_neighbors", "args": { "type_id": 1 } }
        ] }
    ] });
    request["automaton"]["rules"] = json!([{
        "target": "cell",
        "condition": { "kind": "type_is", "args": 0 },
        "probability": probability,
        "actions": [{ "kind": "set_type", "type_id": 1 }]
    }]);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        types(&response["history"][1]["world"]),
        vec![vec![1, 1, 1], vec![1, 1, 1], vec![0, 0, 0]]
    );

    request["generations"] = json!(4);
    request["automaton"]["rules"][0]["probability"]["args"][1]["args"][0]["args"][1]["args"] =
        json!(0.2);
    let (first_status, first) = post(&request).await;
    let (replay_status, replay) = post(&request).await;
    assert_eq!(first_status, StatusCode::OK, "{first}");
    assert_eq!(replay_status, StatusCode::OK, "{replay}");
    assert_eq!(first["seed"], replay["seed"]);
    assert_eq!(first["history"], replay["history"]);
}

#[tokio::test]
async fn reports_probability_definition_errors_before_running_and_dynamic_errors_during_running() {
    for (probability, generations, expected_generation, message) in [
        (
            json!({ "kind": "property", "args": "missing" }),
            0,
            "Generation 0",
            "Unknown property 'missing'",
        ),
        (
            json!({ "kind": "add", "args": [
            { "kind": "constant", "args": 0.6 }, { "kind": "constant", "args": 0.6 }
        ] }),
            0,
            "Generation 0",
            "between 0 and 1",
        ),
        (
            json!({ "kind": "power", "args": [
            { "kind": "constant", "args": -1.0 }, { "kind": "constant", "args": 0.5 }
        ] }),
            0,
            "Generation 0",
            "finite",
        ),
        (
            json!({ "kind": "property", "args": "value" }),
            1,
            "Generation 1: Cell (0, 0)",
            "between 0 and 1",
        ),
        (
            json!({ "kind": "divide", "args": [
            { "kind": "constant", "args": 1.0 },
            { "kind": "subtract", "args": [
                { "kind": "property", "args": "value" }, { "kind": "property", "args": "value" }
            ] }
        ] }),
            1,
            "Generation 1: Cell (0, 0)",
            "Division by zero",
        ),
    ] {
        let mut request = numeric_request();
        request["generations"] = json!(generations);
        request["automaton"]["rules"][0]["probability"] = probability;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains(expected_generation), "{error}");
        assert!(error.contains("Rule 0: probability:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn applies_depth_and_node_limits_to_probability_expressions() {
    let mut deep = json!({ "kind": "constant", "args": 0.0 });
    for _ in 0..17 {
        deep = json!({ "kind": "power", "args": [deep, { "kind": "constant", "args": 1.0 }] });
    }
    let mut wide = json!({ "kind": "constant", "args": 0.0 });
    for _ in 0..9 {
        wide = json!({ "kind": "add", "args": [wide.clone(), wide] });
    }

    for (probability, message) in [(deep, "depth"), (wide, "nodes")] {
        let mut request = numeric_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][0]["probability"] = probability;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}

#[tokio::test]
async fn removes_matching_agents_and_moves_survivors_into_released_positions() {
    let mut request = movement_request();
    request["initial_world"]["agents"][1]["position"]["col"] = json!(1);
    request["automaton"]["rules"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({
                "target": "agent",
                "condition": {
                    "kind": "compare",
                    "args": {
                        "left": { "kind": "property", "args": "step" },
                        "operator": { "kind": "less" },
                        "right": { "kind": "constant", "args": 0 }
                    }
                },
                "probability": 1,
                "actions": [{ "kind": "remove_agent" }]
            }),
        );

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["history"][0]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(response["history"][0]["world"]["agents"][1]["id"], 8);
    for generation in 1..=3 {
        let snapshot = &response["history"][generation];
        assert_eq!(snapshot["world"]["agents"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["world"]["agents"][0]["id"], 7);
        assert_eq!(
            snapshot["world"]["agents"][0]["position"]["col"].as_u64(),
            Some(generation as u64)
        );
        assert_eq!(
            snapshot["world"]["cells"],
            request["initial_world"]["cells"]
        );
    }
}

#[tokio::test]
async fn rejects_invalid_removal_rules_and_unknown_action_fields() {
    let remove = json!({ "kind": "remove_agent" });
    let move_action = movement_request()["automaton"]["rules"][0]["actions"][0].clone();

    for (target, actions, message) in [
        (
            "cell",
            json!([remove.clone()]),
            "RemoveAgent is only allowed in agent rules",
        ),
        (
            "agent",
            json!([remove.clone(), remove.clone()]),
            "RemoveAgent must be the only action",
        ),
        (
            "agent",
            json!([move_action, remove.clone()]),
            "RemoveAgent must be the only action",
        ),
    ] {
        let mut request = movement_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][0]["target"] = json!(target);
        request["automaton"]["rules"][0]["condition"] = json!({ "kind": "always" });
        request["automaton"]["rules"][0]["probability"] = json!(0);
        request["automaton"]["rules"][0]["actions"] = actions;

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 0: Action"), "{error}");
        assert!(error.contains(message), "{error}");
    }

    let mut request = movement_request();
    request["automaton"]["rules"][0]["actions"] = json!([{
        "kind": "remove_agent", "unexpected": true
    }]);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("unknown field"),
        "{response}"
    );
}

#[tokio::test]
async fn keeps_agents_at_zero_steps_and_can_remove_all_agents_during_execution() {
    let mut request = movement_request();
    request["automaton"]["rules"] = json!([{
        "target": "agent",
        "condition": { "kind": "always" },
        "probability": 1,
        "actions": [{ "kind": "remove_agent" }]
    }]);
    request["generations"] = json!(0);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        response["history"][0]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    request["generations"] = json!(3);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"].as_array().unwrap().len(), 4);
    assert_eq!(
        response["history"][0]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for snapshot in &response["history"].as_array().unwrap()[1..] {
        assert_eq!(snapshot["world"]["agents"], json!([]));
        assert_eq!(
            snapshot["world"]["cells"],
            request["initial_world"]["cells"]
        );
    }
}

fn spawning_request() -> Value {
    let mut request = numeric_request();
    request["generations"] = json!(2);
    request["automaton"]["rules"] = json!([
        {
            "target": "cell",
            "condition": { "kind": "always" },
            "probability": 1,
            "actions": [{
                "kind": "spawn_agent",
                "type_id": 0,
                "row_offset": { "kind": "constant", "args": 0 },
                "col_offset": { "kind": "constant", "args": 0 },
                "properties": {
                    "value": { "kind": "divide", "args": [
                        { "kind": "property", "args": "value" },
                        { "kind": "constant", "args": 2 }
                    ] }
                }
            }]
        },
        request["automaton"]["rules"][0].clone()
    ]);
    request["automaton"]["rules"][1]["target"] = json!("agent");
    request
}

#[tokio::test]
async fn accepts_spawn_json_with_computed_properties_or_omitted_overrides() {
    let request = spawning_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert!(
        response["history"][0]["world"]["agents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        response["history"][1]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(response["history"][1]["world"]["agents"][0]["id"], 1);
    assert_eq!(
        response["history"][1]["world"]["agents"][0]["state"]["properties"]["value"].as_f64(),
        Some(49.0)
    );
    assert_eq!(
        response["history"][2]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        response["history"][2]["world"]["agents"][0]["state"]["properties"]["value"].as_f64(),
        Some(50.0)
    );

    let mut defaults = request;
    defaults["generations"] = json!(1);
    defaults["automaton"]["rules"][0]["actions"][0]
        .as_object_mut()
        .unwrap()
        .remove("properties");
    let (status, response) = post(&defaults).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["history"][1]["world"]["agents"][0]["state"]["properties"]["value"].as_f64(),
        Some(0.0)
    );

    defaults["generations"] = json!(0);
    let (status, response) = post(&defaults).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"].as_array().unwrap().len(), 1);
    assert_eq!(response["history"][0]["world"]["agents"], json!([]));
}

#[tokio::test]
async fn validates_spawn_types_properties_offsets_and_action_combinations_before_running() {
    let mut unknown_type = spawning_request();
    unknown_type["automaton"]["rules"][0]["actions"][0]["type_id"] = json!(99);
    let mut unknown_property = spawning_request();
    unknown_property["automaton"]["rules"][0]["actions"][0]["properties"] = json!({
        "missing": { "kind": "constant", "args": 1 }
    });
    let mut bad_constant = spawning_request();
    bad_constant["automaton"]["rules"][0]["actions"][0]["properties"]["value"] =
        json!({ "kind": "constant", "args": 100 });
    let mut fractional = spawning_request();
    fractional["automaton"]["rules"][0]["actions"][0]["row_offset"] =
        json!({ "kind": "constant", "args": 0.5 });
    let mut duplicate = spawning_request();
    let action = duplicate["automaton"]["rules"][0]["actions"][0].clone();
    duplicate["automaton"]["rules"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(action);
    let mut removal = spawning_request();
    removal["automaton"]["rules"][0]["target"] = json!("agent");
    removal["automaton"]["rules"][0]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "kind": "remove_agent" }));

    for (mut request, message) in [
        (unknown_type, "Unknown object type id 99"),
        (unknown_property, "Unknown spawn property 'missing'"),
        (bad_constant, "outside the allowed range"),
        (fractional, "finite integer"),
        (duplicate, "at most one SpawnAgent action"),
        (removal, "RemoveAgent must be the only action"),
    ] {
        request["generations"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 0: Action"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn reports_dynamic_spawn_errors_with_the_generation_cell_and_rule() {
    for (field, expression, message) in [
        (
            "value",
            json!({ "kind": "multiply", "args": [
            { "kind": "property", "args": "value" }, { "kind": "constant", "args": 2 }
        ] }),
            "outside the allowed range",
        ),
        (
            "col_offset",
            json!({ "kind": "divide", "args": [
            { "kind": "property", "args": "value" }, { "kind": "constant", "args": 4 }
        ] }),
            "finite integer",
        ),
    ] {
        let mut request = spawning_request();
        if field == "value" {
            request["automaton"]["rules"][0]["actions"][0]["properties"][field] = expression;
        } else {
            request["automaton"]["rules"][0]["actions"][0][field] = expression;
        }
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(
            error.contains("Generation 1: Cell (0, 0): Rule 0: SpawnAgent"),
            "{error}"
        );
        assert!(error.contains(message), "{error}");
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn limits_expressions_in_spawn_offsets_and_initial_properties() {
    let mut deep = json!({ "kind": "constant", "args": 0 });
    for _ in 0..17 {
        deep = json!({ "kind": "add", "args": [deep, { "kind": "constant", "args": 0 }] });
    }
    let mut wide = json!({ "kind": "constant", "args": 0 });
    for _ in 0..9 {
        wide = json!({ "kind": "add", "args": [wide.clone(), wide] });
    }
    for (field, expression, message) in [
        ("row_offset", deep.clone(), "depth"),
        ("col_offset", deep.clone(), "depth"),
        ("value", deep, "depth"),
        ("value", wide, "nodes"),
    ] {
        let mut request = spawning_request();
        request["generations"] = json!(0);
        if field == "value" {
            request["automaton"]["rules"][0]["actions"][0]["properties"][field] = expression;
        } else {
            request["automaton"]["rules"][0]["actions"][0][field] = expression;
        }
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}

#[tokio::test]
async fn applies_history_limits_to_retained_generations_including_births() {
    let mut request = spawning_request();
    request["generations"] = json!(300);
    request["initial_world"]["cells"] = json!([vec![
        json!({ "type_id": 0, "properties": { "value": 98 } });
        100
    ]]);
    request["automaton"]["rules"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    request["automaton"]["rules"][0]["actions"][0]["properties"] = json!({});

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let error = response["error"].as_str().unwrap();
    assert!(
        error.contains("Generation 250: Simulation history exceeds 100000"),
        "{error}"
    );
    assert!(response.get("history").is_none());

    request["save_history"] = json!(false);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"].as_array().unwrap().len(), 1);
    assert_eq!(response["history"][0]["generation"], 300);
    assert_eq!(
        response["history"][0]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
}

fn terrain_request() -> Value {
    let mut request = numeric_request();
    let property = request["automaton"]["object_types"][0]["properties"]["value"].clone();
    request["automaton"]["name"] = json!("Reading terrain");
    request["automaton"]["object_types"] = json!([
        { "id": 0, "name": "soil", "properties": { "food": property.clone() } },
        { "id": 1, "name": "rabbit", "properties": { "energy": property } },
        { "id": 2, "name": "rock", "properties": {} }
    ]);
    request["automaton"]["neighborhood"] = json!({ "offsets": [] });
    request["automaton"]["rules"] = json!([
        {
            "target": "cell",
            "condition": { "kind": "type_is", "args": 0 },
            "probability": 1,
            "actions": [{ "kind": "set_property", "property": "food", "value": { "kind": "constant", "args": 0 } }]
        },
        {
            "target": "agent",
            "condition": { "kind": "all", "args": [
                { "kind": "type_is", "args": 1 },
                { "kind": "cell_type_is", "args": 0 },
                { "kind": "compare", "args": {
                    "left": { "kind": "cell_property", "args": "food" },
                    "operator": { "kind": "greater" },
                    "right": { "kind": "constant", "args": 0 }
                } }
            ] },
            "probability": 1,
            "actions": [
                { "kind": "set_property", "property": "energy", "value": { "kind": "add", "args": [
                    { "kind": "property", "args": "energy" },
                    { "kind": "cell_property", "args": "food" }
                ] } },
                { "kind": "move", "row_offset": { "kind": "constant", "args": 0 }, "col_offset": { "kind": "constant", "args": 1 } }
            ]
        }
    ]);
    request["initial_world"] = json!({
        "cells": [[
            { "type_id": 0, "properties": { "food": 3 } },
            { "type_id": 0, "properties": { "food": 7 } },
            { "type_id": 2, "properties": {} }
        ]],
        "agents": [
            { "id": 7, "position": { "row": 0, "col": 0 }, "state": { "type_id": 1, "properties": { "energy": 10 } } },
            { "id": 9, "position": { "row": 0, "col": 2 }, "state": { "type_id": 1, "properties": { "energy": 20 } } }
        ]
    });
    request["generations"] = json!(2);
    request
}

#[tokio::test]
async fn reads_the_original_cell_under_each_agent_through_json_conditions_and_actions() {
    let request = terrain_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"].as_array().unwrap().len(), 3);
    assert_eq!(
        response["history"][0]["world"]["cells"][0][0]["properties"]["food"].as_f64(),
        Some(3.0)
    );
    for generation in 1..=2 {
        let world = &response["history"][generation]["world"];
        assert_eq!(
            world["cells"][0][0]["properties"]["food"].as_f64(),
            Some(0.0)
        );
        assert_eq!(
            world["agents"][0]["state"]["properties"]["energy"].as_f64(),
            Some(13.0)
        );
        assert_eq!(world["agents"][0]["position"]["col"], 1);
        assert_eq!(
            world["agents"][1]["state"]["properties"]["energy"].as_f64(),
            Some(20.0)
        );
        assert_eq!(world["agents"][1]["position"]["col"], 2);
    }
}

#[tokio::test]
async fn rejects_unknown_cell_references_even_before_a_zero_step_run() {
    let mut wrong_type = terrain_request();
    wrong_type["automaton"]["rules"][1]["condition"] = json!({ "kind": "any", "args": [
        { "kind": "always" }, { "kind": "cell_type_is", "args": 99 }
    ] });
    let mut wrong_condition = terrain_request();
    wrong_condition["automaton"]["rules"][1]["condition"]["args"][2]["args"]["left"] =
        json!({ "kind": "cell_property", "args": "missing" });
    let mut wrong_probability = terrain_request();
    wrong_probability["automaton"]["rules"][1]["probability"] =
        json!({ "kind": "cell_property", "args": "missing" });
    let mut wrong_action = terrain_request();
    wrong_action["automaton"]["rules"][1]["actions"][0]["value"] =
        json!({ "kind": "cell_property", "args": "missing" });

    for (mut request, message) in [
        (wrong_type, "Unknown object type id 99"),
        (wrong_condition, "Unknown property 'missing'"),
        (wrong_probability, "Unknown property 'missing'"),
        (wrong_action, "Unknown property 'missing'"),
    ] {
        request["generations"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 1:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn reports_actual_cell_property_and_probability_errors_without_partial_history() {
    let mut unguarded = terrain_request();
    unguarded["automaton"]["rules"][1]["condition"] = json!({ "kind": "type_is", "args": 1 });
    let mut invalid_probability = terrain_request();
    invalid_probability["automaton"]["rules"][1]["probability"] =
        json!({ "kind": "cell_property", "args": "food" });

    for (request, location, message) in [
        (
            unguarded,
            "Generation 1: Agent 9: Rule 1:",
            "Current cell is missing property 'food'",
        ),
        (
            invalid_probability,
            "Generation 1: Agent 7: Rule 1: probability:",
            "between 0 and 1",
        ),
    ] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains(location), "{error}");
        assert!(error.contains(message), "{error}");
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn includes_cell_type_and_property_reads_in_rule_size_limits() {
    let mut too_many = terrain_request();
    too_many["generations"] = json!(0);
    too_many["automaton"]["rules"][1]["condition"] =
        json!({ "kind": "all", "args": vec![json!({ "kind": "cell_type_is", "args": 0 }); 512] });
    let mut too_deep = terrain_request();
    too_deep["generations"] = json!(0);
    let mut expression = json!({ "kind": "cell_property", "args": "food" });
    for _ in 0..17 {
        expression =
            json!({ "kind": "add", "args": [expression, { "kind": "constant", "args": 0 }] });
    }
    too_deep["automaton"]["rules"][1]["actions"][0]["value"] = expression;

    for (request, message) in [(too_many, "nodes"), (too_deep, "depth")] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}

fn consumption_request() -> Value {
    let mut request = terrain_request();
    request["automaton"]["rules"][0]["actions"][0]["value"] = json!({ "kind": "add", "args": [
        { "kind": "cell_property", "args": "food" }, { "kind": "constant", "args": 1 }
    ] });
    request["automaton"]["rules"][1]["actions"].as_array_mut().unwrap().insert(1, json!({
        "kind": "set_cell_property", "property": "food", "value": { "kind": "constant", "args": 0 }
    }));
    request
}

#[tokio::test]
async fn consumes_food_and_updates_energy_while_preserving_history_and_other_cell_rules() {
    let request = consumption_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    let history = response["history"].as_array().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0]["world"]["cells"][0][0]["properties"]["food"].as_f64(),
        Some(3.0)
    );
    assert_eq!(
        history[1]["world"]["cells"][0][0]["properties"]["food"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        history[1]["world"]["cells"][0][1]["properties"]["food"].as_f64(),
        Some(8.0)
    );
    assert_eq!(
        history[1]["world"]["agents"][0]["state"]["properties"]["energy"].as_f64(),
        Some(13.0)
    );
    assert_eq!(history[1]["world"]["agents"][0]["position"]["col"], 1);
    assert_eq!(
        history[2]["world"]["cells"][0][0]["properties"]["food"].as_f64(),
        Some(1.0)
    );
    assert_eq!(
        history[2]["world"]["cells"][0][1]["properties"]["food"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        history[2]["world"]["agents"][0]["state"]["properties"]["energy"].as_f64(),
        Some(21.0)
    );
    assert_eq!(history[2]["world"]["agents"][0]["position"]["col"], 1);
    assert_eq!(
        history[2]["world"]["agents"][1]["state"]["properties"]["energy"].as_f64(),
        Some(20.0)
    );
}

#[tokio::test]
async fn changes_terrain_type_and_replaces_its_properties_without_changing_agent_type() {
    let mut request = terrain_request();
    request["automaton"]["rules"][1]["actions"] =
        json!([{ "kind": "set_cell_type", "type_id": 2 }]);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["history"][0]["world"]["cells"][0][0]["type_id"], 0);
    for generation in 1..=2 {
        let world = &response["history"][generation]["world"];
        assert_eq!(
            world["cells"][0][0],
            json!({ "type_id": 2, "properties": {} })
        );
        assert_eq!(world["agents"][0]["state"]["type_id"], 1);
        assert_eq!(
            world["agents"][0]["state"]["properties"]["energy"].as_f64(),
            Some(10.0)
        );
    }
}

#[tokio::test]
async fn validates_cell_writing_targets_types_and_property_references_before_running() {
    let mut cell_property = consumption_request();
    cell_property["automaton"]["rules"][0]["actions"] = json!([{ "kind": "set_cell_property", "property": "food", "value": { "kind": "constant", "args": 0 } }]);
    let mut cell_type = consumption_request();
    cell_type["automaton"]["rules"][0]["actions"] =
        json!([{ "kind": "set_cell_type", "type_id": 2 }]);
    let mut unknown_type = consumption_request();
    unknown_type["automaton"]["rules"][1]["actions"] =
        json!([{ "kind": "set_cell_type", "type_id": 99 }]);
    let mut wrong_property = consumption_request();
    wrong_property["automaton"]["rules"][1]["actions"] = json!([
        { "kind": "set_cell_type", "type_id": 2 },
        { "kind": "set_cell_property", "property": "food", "value": { "kind": "constant", "args": 0 } }
    ]);
    let mut unknown_property = consumption_request();
    unknown_property["automaton"]["rules"][1]["actions"][1]["property"] = json!("missing");
    let mut unknown_expression = consumption_request();
    unknown_expression["automaton"]["rules"][1]["actions"][1]["value"] =
        json!({ "kind": "cell_property", "args": "missing" });

    for (mut request, message) in [
        (
            cell_property,
            "SetCellProperty is only allowed in agent rules",
        ),
        (cell_type, "SetCellType is only allowed in agent rules"),
        (unknown_type, "Unknown object type id 99"),
        (wrong_property, "Unknown cell property 'food' for type 2"),
        (unknown_property, "Unknown property 'missing'"),
        (unknown_expression, "Unknown property 'missing'"),
    ] {
        request["generations"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn reports_cell_write_conflicts_bounds_and_changed_schema_errors_without_partial_results() {
    let mut conflict = consumption_request();
    conflict["initial_world"]["agents"][1]["position"]["col"] = json!(0);
    let mut bounds = consumption_request();
    bounds["automaton"]["rules"][1]["actions"][1]["value"] =
        json!({ "kind": "constant", "args": 100 });
    let mut schema = consumption_request();
    schema["automaton"]["rules"][0]["actions"] = json!([{ "kind": "set_type", "type_id": 2 }]);

    for (request, location, message) in [
        (
            conflict,
            "Generation 1: Cell (0, 0):",
            "Conflicting cell updates from agents 7 and 9",
        ),
        (
            bounds,
            "Generation 1: Agent 7: Rule 1:",
            "outside the allowed range",
        ),
        (
            schema,
            "Generation 1: Cell (0, 0): Agent 7:",
            "Unknown cell property 'food' for type 2",
        ),
    ] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains(location), "{error}");
        assert!(error.contains(message), "{error}");
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn applies_expression_depth_and_node_limits_to_cell_writing_actions() {
    let mut deep = json!({ "kind": "cell_property", "args": "food" });
    for _ in 0..17 {
        deep = json!({ "kind": "add", "args": [deep, { "kind": "constant", "args": 0 }] });
    }
    let mut wide = json!({ "kind": "constant", "args": 0 });
    for _ in 0..9 {
        wide = json!({ "kind": "add", "args": [wide.clone(), wide] });
    }
    for (expression, message) in [(deep, "depth"), (wide, "nodes")] {
        let mut request = consumption_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][1]["actions"][1]["value"] = expression;
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}

fn agent_observation_request() -> Value {
    let mut request = numeric_request();
    let property = request["automaton"]["object_types"][0]["properties"]["value"].clone();
    request["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 1, "name": "rabbit", "properties": { "energy": property }
        }));
    request["automaton"]["neighborhood"] = json!({ "offsets": [[-1, 0], [1, 0], [0, -1], [0, 1]] });
    request["automaton"]["rules"] = json!([
        {
            "target": "cell", "condition": { "kind": "always" }, "probability": 1,
            "actions": [{ "kind": "set_property", "property": "value", "value": { "kind": "count_neighbor_agents", "args": { "type_id": 1 } } }]
        },
        {
            "target": "agent",
            "condition": { "kind": "all", "args": [
                { "kind": "type_is", "args": 1 },
                { "kind": "compare", "args": {
                    "left": { "kind": "count_neighbor_agents", "args": { "type_id": 1 } },
                    "operator": { "kind": "equal" },
                    "right": { "kind": "constant", "args": 1 }
                } }
            ] },
            "probability": { "kind": "count_neighbor_agents", "args": { "type_id": 1 } },
            "actions": [{ "kind": "set_property", "property": "energy", "value": { "kind": "sum_neighbor_agents", "args": { "type_id": 1, "property": "energy" } } }]
        }
    ]);
    request["initial_world"] = json!({
        "cells": [[{ "type_id": 0, "properties": { "value": 0 } }, { "type_id": 0, "properties": { "value": 0 } }]],
        "agents": [
            { "id": 7, "position": { "row": 0, "col": 0 }, "state": { "type_id": 1, "properties": { "energy": 10 } } },
            { "id": 9, "position": { "row": 0, "col": 1 }, "state": { "type_id": 1, "properties": { "energy": 20 } } }
        ]
    });
    request["generations"] = json!(2);
    request
}

#[tokio::test]
async fn observes_neighbor_agents_in_conditions_probabilities_actions_and_cell_rules() {
    let request = agent_observation_request();

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::OK, "{response}");
    for (generation, expected) in [(1, [20.0, 10.0]), (2, [10.0, 20.0])] {
        let world = &response["history"][generation]["world"];
        for (index, energy) in expected.into_iter().enumerate() {
            assert_eq!(
                world["agents"][index]["state"]["properties"]["energy"].as_f64(),
                Some(energy)
            );
            assert_eq!(
                world["cells"][0][index]["properties"]["value"].as_f64(),
                Some(1.0)
            );
        }
    }
    let (replay_status, replay) = post(&request).await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(response["seed"], replay["seed"]);
    assert_eq!(response["history"], replay["history"]);
}

#[tokio::test]
async fn excludes_the_current_position_for_agent_observations_on_one_cell_worlds() {
    for boundary in [
        json!({ "kind": "wrap" }),
        json!({ "kind": "fixed", "state": { "type_id": 0, "properties": { "value": 0 } } }),
    ] {
        let mut request = agent_observation_request();
        request["automaton"]["boundary"] = boundary;
        request["initial_world"]["cells"].as_array_mut().unwrap()[0]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        request["initial_world"]["agents"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        request["automaton"]["rules"][1]["condition"] = json!({ "kind": "type_is", "args": 1 });
        request["automaton"]["rules"][1]["actions"][0]["value"] =
            json!({ "kind": "constant", "args": 99 });

        let (status, response) = post(&request).await;

        assert_eq!(status, StatusCode::OK, "{response}");
        assert_eq!(
            response["history"][1]["world"]["agents"][0]["state"]["properties"]["energy"].as_f64(),
            Some(10.0)
        );
        assert_eq!(
            response["history"][1]["world"]["cells"][0][0]["properties"]["value"].as_f64(),
            Some(0.0)
        );
    }
}

#[tokio::test]
async fn validates_agent_observation_types_and_type_specific_properties_at_zero_steps() {
    let mut wrong_count = agent_observation_request();
    wrong_count["automaton"]["rules"][1]["condition"]["args"][1]["args"]["left"]["args"]["type_id"] =
        json!(99);
    let mut wrong_sum_type = agent_observation_request();
    wrong_sum_type["automaton"]["rules"][1]["actions"][0]["value"]["args"]["type_id"] = json!(99);
    let mut property_on_another_type = agent_observation_request();
    property_on_another_type["automaton"]["rules"][1]["actions"][0]["value"]["args"]["property"] =
        json!("value");
    let mut unknown_property = agent_observation_request();
    unknown_property["automaton"]["rules"][1]["actions"][0]["value"]["args"]["property"] =
        json!("missing");

    for (mut request, message) in [
        (wrong_count, "Unknown object type id 99"),
        (wrong_sum_type, "Unknown object type id 99"),
        (
            property_on_another_type,
            "Unknown property 'value' for neighbor agent type 1",
        ),
        (
            unknown_property,
            "Unknown property 'missing' for neighbor agent type 1",
        ),
    ] {
        request["generations"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 1:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn counts_each_agent_on_a_shared_neighbor_position_and_checks_probability_range() {
    let mut request = agent_observation_request();
    request["automaton"]["rules"][1]["condition"] = json!({ "kind": "type_is", "args": 1 });
    let mut other = request["initial_world"]["agents"][1].clone();
    other["id"] = json!(11);
    request["initial_world"]["agents"]
        .as_array_mut()
        .unwrap()
        .push(other);

    let (status, response) = post(&request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let error = response["error"].as_str().unwrap();
    assert!(
        error.contains("Generation 1: Agent 7: Rule 1: probability:"),
        "{error}"
    );
    assert!(error.contains("between 0 and 1"), "{error}");
    assert!(response.get("history").is_none());
}

#[tokio::test]
async fn includes_agent_observation_expressions_in_rule_depth_and_node_limits() {
    let mut deep =
        json!({ "kind": "sum_neighbor_agents", "args": { "type_id": 1, "property": "energy" } });
    for _ in 0..17 {
        deep = json!({ "kind": "add", "args": [deep, { "kind": "constant", "args": 0 }] });
    }
    let mut wide = json!({ "kind": "count_neighbor_agents", "args": { "type_id": 1 } });
    for _ in 0..9 {
        wide = json!({ "kind": "add", "args": [wide.clone(), wide] });
    }
    for (expression, message) in [(deep, "depth"), (wide, "nodes")] {
        let mut request = agent_observation_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][1]["probability"] = expression;
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}

fn directional_request() -> Value {
    let mut request = numeric_request();
    request["generations"] = json!(2);
    request["initial_world"]["cells"] = json!([[
        { "type_id": 0, "properties": { "value": 3.25 } },
        { "type_id": 0, "properties": { "value": 7.75 } }
    ]]);
    request["automaton"]["rules"][0]["condition"] = json!({
        "kind": "neighbor_type_is", "args": { "offset": [0, 1], "type_id": 0 }
    });
    let value =
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 1], "property": "value" } });
    request["automaton"]["rules"][0]["probability"] =
        json!({ "kind": "divide", "args": [value.clone(), value.clone()] });
    request["automaton"]["rules"][0]["actions"][0]["value"] = value;
    request
}

fn free_movement_request(selection: &str) -> Value {
    let mut request = movement_request();
    request["automaton"]["rules"][0]["actions"] = json!([{
        "kind": "move_to_free_neighbor", "selection": selection, "cell_type": 0
    }]);
    request
}

fn predator_prey_request() -> Value {
    serde_json::from_value(common::captures()).unwrap()
}

#[tokio::test]
async fn predator_prey_json_links_energy_rewards_to_successful_captures_and_replays() {
    let mut request = predator_prey_request();
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["history"][1]["world"]["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for pair in response["history"].as_array().unwrap().windows(2) {
        let old = pair[0]["world"]["agents"].as_array().unwrap();
        let next = pair[1]["world"]["agents"].as_array().unwrap();
        let old_prey = old
            .iter()
            .filter(|agent| agent["state"]["type_id"] == 1)
            .count();
        let next_prey = next
            .iter()
            .filter(|agent| agent["state"]["type_id"] == 1)
            .count();
        let old_energy = old.iter().find(|agent| agent["id"] == 2).unwrap()["state"]["properties"]
            ["energy"]
            .as_f64()
            .unwrap();
        let next_energy =
            next.iter().find(|agent| agent["id"] == 2).unwrap()["state"]["properties"]["energy"]
                .as_f64()
                .unwrap();
        assert!(old_prey == next_prey || old_prey == next_prey + 1);
        assert_eq!(
            next_energy,
            if old_prey > next_prey {
                old_energy + 10.0
            } else {
                old_energy - 1.0
            }
        );
    }
    let (status, replay) = post(&request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["history"], replay["history"]);
    request["save_history"] = json!(false);
    let (status, final_only) = post(&request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(final_only["history"].as_array().unwrap().len(), 1);
    assert_eq!(final_only["history"][0], response["history"][5]);
}

#[tokio::test]
async fn competing_predators_do_not_remove_or_both_gain_energy_from_one_prey() {
    let mut request = predator_prey_request();
    request["generations"] = json!(1);
    request["automaton"]["boundary"] =
        json!({ "kind": "fixed", "state": { "type_id": 0, "properties": {} } });
    request["initial_world"]["cells"].as_array_mut().unwrap()[0]
        .as_array_mut()
        .unwrap()
        .truncate(3);
    let predator = request["initial_world"]["agents"][1].clone();
    let prey = request["initial_world"]["agents"][0].clone();
    let mut competitor = predator.clone();
    competitor["id"] = json!(4);
    competitor["position"]["col"] = json!(2);
    request["initial_world"]["agents"] = json!([predator, prey, competitor]);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let agents = response["history"][1]["world"]["agents"]
        .as_array()
        .unwrap();
    assert_eq!(agents.len(), 3);
    for (index, agent) in agents.iter().enumerate() {
        assert_eq!(
            agent["position"],
            request["initial_world"]["agents"][index]["position"]
        );
        if agent["state"]["type_id"] == 2 {
            assert_eq!(agent["state"]["properties"]["energy"].as_f64(), Some(10.0));
        }
    }
}

#[tokio::test]
async fn missing_neighbor_targets_skip_rewards_even_when_removal_is_the_last_action() {
    let mut request = predator_prey_request();
    let predator = request["initial_world"]["agents"][1].clone();
    request["initial_world"]["agents"] = json!([predator]);
    request["automaton"]["rules"][1]["condition"] = json!({ "kind": "type_is", "args": 2 });
    request["automaton"]["rules"][1]["actions"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    for snapshot in response["history"].as_array().unwrap() {
        let agent = &snapshot["world"]["agents"][0];
        assert_eq!(agent["state"]["properties"]["energy"].as_f64(), Some(10.0));
        assert_eq!(agent["position"]["col"], 0);
    }
}

#[tokio::test]
async fn validates_neighbor_removal_schema_targets_types_and_counts_before_running() {
    let base = predator_prey_request();
    let mut unknown_type = base.clone();
    unknown_type["automaton"]["rules"][1]["actions"][0]["type_id"] = json!(99);
    let mut cell_target = base.clone();
    cell_target["automaton"]["rules"][1]["target"] = json!("cell");
    let mut invalid_selection = base.clone();
    invalid_selection["automaton"]["rules"][1]["actions"][0]["selection"] = json!("nearest");
    let mut missing_type = base.clone();
    missing_type["automaton"]["rules"][1]["actions"][0]
        .as_object_mut()
        .unwrap()
        .remove("type_id");
    let mut unknown_field = base.clone();
    unknown_field["automaton"]["rules"][1]["actions"][0]["typo"] = json!(true);
    let mut repeated = base.clone();
    let action = repeated["automaton"]["rules"][1]["actions"][0].clone();
    repeated["automaton"]["rules"][1]["actions"]
        .as_array_mut()
        .unwrap()
        .push(action);
    let mut self_removal = base.clone();
    self_removal["automaton"]["rules"][1]["actions"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "kind": "remove_agent" }));
    for mut request in [
        unknown_type,
        cell_target,
        invalid_selection,
        missing_type,
        unknown_field,
        repeated,
        self_removal,
    ] {
        request["generations"] = json!(0);
        request["automaton"]["rules"][1]["probability"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].is_string());
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn invalid_energy_rewards_report_the_agent_and_generation_without_partial_results() {
    let mut request = predator_prey_request();
    request["initial_world"]["agents"][1]["state"]["properties"]["energy"] = json!(95);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    let error = response["error"].as_str().unwrap();
    assert!(error.contains("Generation 1: Agent 2: Rule 1:"), "{error}");
    assert!(error.contains("energy"), "{error}");
    assert!(response.get("history").is_none());
}

#[tokio::test]
async fn chooses_and_recalculates_first_free_destinations_through_json() {
    let request = free_movement_request("first");
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    for (generation, columns) in [(1, [1, 3]), (2, [0, 2]), (3, [4, 1])] {
        for (index, col) in columns.into_iter().enumerate() {
            assert_eq!(
                response["history"][generation]["world"]["agents"][index]["position"]["col"],
                col
            );
        }
    }
}

#[tokio::test]
async fn random_free_movement_replays_and_agrees_with_final_only_http_runs() {
    let mut request = free_movement_request("random");
    request["generations"] = json!(16);
    request["automaton"]["rules"][0]["actions"][0]
        .as_object_mut()
        .unwrap()
        .remove("cell_type");
    let (status, first) = post(&request).await;
    let (replay_status, replay) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(first["history"], replay["history"]);
    request["save_history"] = json!(false);
    let (status, final_only) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{final_only}");
    assert_eq!(final_only["history"].as_array().unwrap().len(), 1);
    assert_eq!(final_only["history"][0], first["history"][16]);
}

#[tokio::test]
async fn no_matching_terrain_or_empty_neighborhood_keeps_agents_in_place() {
    let mut no_matching_type = free_movement_request("random");
    no_matching_type["automaton"]["rules"][0]["actions"][0]["cell_type"] = json!(1);
    let mut empty_neighborhood = free_movement_request("random");
    empty_neighborhood["automaton"]["neighborhood"]["offsets"] = json!([]);
    for request in [no_matching_type, empty_neighborhood] {
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        for snapshot in response["history"].as_array().unwrap() {
            for index in 0..2 {
                assert_eq!(
                    snapshot["world"]["agents"][index]["position"],
                    request["initial_world"]["agents"][index]["position"]
                );
            }
        }
    }
}

#[tokio::test]
async fn validates_free_movement_targets_filters_selection_and_action_count_at_zero_steps() {
    let base = free_movement_request("first");
    let free = base["automaton"]["rules"][0]["actions"][0].clone();
    let explicit = movement_request()["automaton"]["rules"][0]["actions"][0].clone();
    let mut cell_target = base.clone();
    cell_target["automaton"]["rules"][0]["target"] = json!("cell");
    let mut unknown_type = base.clone();
    unknown_type["automaton"]["rules"][0]["actions"][0]["cell_type"] = json!(99);
    let invalid_selection = free_movement_request("nearest");
    let mut missing_selection = base.clone();
    missing_selection["automaton"]["rules"][0]["actions"][0]
        .as_object_mut()
        .unwrap()
        .remove("selection");
    let mut unknown_field = base.clone();
    unknown_field["automaton"]["rules"][0]["actions"][0]["typo"] = json!(true);
    let mut cases = vec![
        cell_target,
        unknown_type,
        invalid_selection,
        missing_selection,
        unknown_field,
    ];
    for actions in [
        json!([free.clone(), explicit.clone()]),
        json!([explicit, free.clone()]),
        json!([free.clone(), free]),
    ] {
        let mut duplicate = base.clone();
        duplicate["automaton"]["rules"][0]["actions"] = actions;
        cases.push(duplicate);
    }
    for mut request in cases {
        request["generations"] = json!(0);
        request["automaton"]["rules"][0]["probability"] = json!(0);
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].is_string());
        assert!(response.get("history").is_none());
    }
}

#[tokio::test]
async fn runs_rule_30_from_json_through_the_simulation_endpoint() {
    let mut request: Value = serde_json::from_value(common::directional_cells()).unwrap();
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let expected = [
        "00000100000",
        "00001110000",
        "00011001000",
        "00110111100",
        "01100100010",
    ];
    assert_eq!(
        response["history"].as_array().unwrap().len(),
        expected.len()
    );
    for (generation, row) in expected.into_iter().enumerate() {
        let row: Vec<u64> = row.bytes().map(|value| u64::from(value - b'0')).collect();
        assert_eq!(response["history"][generation]["generation"], generation);
        assert_eq!(types(&response["history"][generation]["world"]), vec![row]);
    }
    request["save_history"] = json!(false);
    let (status, final_only) = post(&request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(final_only["history"].as_array().unwrap().len(), 1);
    assert_eq!(final_only["history"][0], response["history"][4]);
}

#[tokio::test]
async fn reads_directional_properties_in_conditions_probabilities_and_assignments() {
    let request = directional_request();
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let cells = &response["history"][1]["world"]["cells"][0];
    assert_eq!(cells[0]["properties"]["value"].as_f64(), Some(7.75));
    assert_eq!(cells[1]["properties"]["value"].as_f64(), Some(3.25));
    assert_eq!(response["history"][2]["world"], request["initial_world"]);
}

#[tokio::test]
async fn neighbor_type_guards_avoid_missing_fixed_boundary_properties() {
    let mut request = directional_request();
    request["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 1, "name": "rock", "properties": {}
        }));
    request["automaton"]["boundary"] =
        json!({ "kind": "fixed", "state": { "type_id": 1, "properties": {} } });

    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["history"][2]["world"]["cells"][0][1]["properties"]["value"].as_f64(),
        Some(7.75)
    );

    request["automaton"]["rules"][0]["condition"] = json!({ "kind": "always" });
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let error = response["error"].as_str().unwrap();
    assert!(error.contains("Generation 1: Cell (0, 1)"), "{error}");
    assert!(error.contains("offset (0, 1)"), "{error}");
    assert!(error.contains("missing property 'value'"), "{error}");
    assert!(response.get("history").is_none());
}

#[tokio::test]
async fn validates_directional_offsets_types_and_properties_before_zero_step_runs() {
    let valid_value =
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 1], "property": "value" } });
    for (condition, value, message) in [
        (
            json!({ "kind": "neighbor_type_is", "args": { "offset": [0, -1], "type_id": 0 } }),
            valid_value.clone(),
            "offset (0, -1)",
        ),
        (
            json!({ "kind": "neighbor_type_is", "args": { "offset": [0, 1], "type_id": 99 } }),
            valid_value.clone(),
            "Unknown object type id 99",
        ),
        (
            json!({ "kind": "always" }),
            json!({ "kind": "neighbor_property", "args": { "offset": [0, -1], "property": "value" } }),
            "offset (0, -1)",
        ),
        (
            json!({ "kind": "always" }),
            json!({ "kind": "neighbor_property", "args": { "offset": [0, 0], "property": "value" } }),
            "offset (0, 0)",
        ),
        (
            json!({ "kind": "always" }),
            json!({ "kind": "neighbor_property", "args": { "offset": [0, 1], "property": "missing" } }),
            "Unknown property 'missing'",
        ),
    ] {
        let mut request = directional_request();
        request["generations"] = json!(0);
        request["automaton"]["rules"][0]["condition"] =
            json!({ "kind": "any", "args": [{ "kind": "always" }, condition] });
        request["automaton"]["rules"][0]["probability"] = json!(0);
        request["automaton"]["rules"][0]["actions"][0]["value"] = value;
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = response["error"].as_str().unwrap();
        assert!(error.contains("Generation 0: Rule 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[tokio::test]
async fn rejects_malformed_directional_offsets_and_unknown_fields() {
    for value in [
        json!({ "kind": "neighbor_property", "args": { "offset": [0], "property": "value" } }),
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 1.5], "property": "value" } }),
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 2147483648_u64], "property": "value" } }),
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 1], "property": "value", "typo": true } }),
    ] {
        let mut request = directional_request();
        request["automaton"]["rules"][0]["actions"][0]["value"] = value;
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(response["error"].is_string());
    }
    let mut request = directional_request();
    request["automaton"]["rules"][0]["condition"]["args"]["typo"] = json!(true);
    let (status, response) = post(&request).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
}

#[tokio::test]
async fn includes_directional_reads_in_rule_depth_and_node_limits() {
    let leaf = json!({ "kind": "neighbor_type_is", "args": { "offset": [0, 1], "type_id": 0 } });
    let mut deep_condition = leaf.clone();
    for _ in 0..17 {
        deep_condition = json!({ "kind": "not", "args": deep_condition });
    }
    let wide_condition = json!({ "kind": "all", "args": vec![leaf; 513] });
    let mut deep_value =
        json!({ "kind": "neighbor_property", "args": { "offset": [0, 1], "property": "value" } });
    let mut wide_value = deep_value.clone();
    for _ in 0..17 {
        deep_value =
            json!({ "kind": "add", "args": [deep_value, { "kind": "constant", "args": 0 }] });
    }
    for _ in 0..9 {
        wide_value = json!({ "kind": "add", "args": [wide_value.clone(), wide_value] });
    }
    for (condition, value, message) in [
        (Some(deep_condition), None, "depth"),
        (Some(wide_condition), None, "nodes"),
        (None, Some(deep_value), "depth"),
        (None, Some(wide_value), "nodes"),
    ] {
        let mut request = directional_request();
        request["generations"] = json!(0);
        if let Some(condition) = condition {
            request["automaton"]["rules"][0]["condition"] = condition;
        }
        if let Some(value) = value {
            request["automaton"]["rules"][0]["actions"][0]["value"] = value;
        }
        let (status, response) = post(&request).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert!(
            response["error"].as_str().unwrap().contains(message),
            "{response}"
        );
    }
}
