use backend::{
    models::{automaton::Boundary, simulation::SimulationRequest, state::ObjectState},
    simulator::run,
};
use serde_json::{Value, json};

fn request() -> SimulationRequest {
    serde_json::from_str(include_str!("../examples/sandpile.json")).unwrap()
}

fn numeric_grid(request: &mut SimulationRequest, values: &[&[f64]]) {
    request.initial_world.cells = values
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| ObjectState {
                    type_id: 0,
                    properties: [("grains".to_owned(), *value)].into(),
                })
                .collect()
        })
        .collect();
}

fn grains(request: &SimulationRequest, generations: usize) -> Vec<Vec<Vec<f64>>> {
    run(
        &request.automaton,
        &request.initial_world,
        generations,
        request.seed,
        true,
    )
    .unwrap()
    .into_iter()
    .map(|frame| {
        frame
            .world
            .cells
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| cell.properties["grains"])
                    .collect()
            })
            .collect()
    })
    .collect()
}

#[test]
fn simultaneous_topplings_accumulate_and_only_affect_the_following_generation() {
    let mut request = request();
    numeric_grid(&mut request, &[&[0., 4., 0.], &[4., 0., 4.], &[0., 4., 0.]]);
    let history = grains(&request, 2);
    assert_eq!(
        history[1],
        vec![vec![2., 0., 2.], vec![0., 4., 0.], vec![2., 0., 2.]]
    );
    assert_eq!(
        history[2],
        vec![vec![2., 1., 2.], vec![1., 0., 1.], vec![2., 1., 2.]]
    );
    assert_eq!(request.initial_world.cells[1][1].properties["grains"], 0.);
}

#[test]
fn wrapped_contributions_preserve_each_direction_even_when_positions_alias() {
    let mut request = request();
    request.automaton.boundary = Boundary::Wrap;
    for values in [vec![vec![4.]], vec![vec![4., 4.]]] {
        numeric_grid(
            &mut request,
            &values.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        );
        for frame in grains(&request, 3) {
            assert_eq!(frame, values);
        }
    }
}

#[test]
fn closed_sandpile_conserves_grains_and_relaxes_when_the_initial_pile_allows_it() {
    let mut request = request();
    request.automaton.boundary = Boundary::Wrap;
    let history = grains(&request, 6);
    for frame in &history {
        assert_eq!(frame.iter().flatten().sum::<f64>(), 8.);
        assert!(
            frame
                .iter()
                .flatten()
                .all(|value| *value >= 0. && value.fract() == 0.)
        );
    }
    assert!(
        history
            .last()
            .unwrap()
            .iter()
            .flatten()
            .all(|value| *value < 4.)
    );
    assert_eq!(history[5], history[6]);
}

#[test]
fn fixed_boundary_discards_outward_contributions_without_changing_the_boundary() {
    let mut request = request();
    numeric_grid(&mut request, &[&[4., 0.], &[0., 0.]]);
    let before = serde_json::to_value(&request.automaton.boundary).unwrap();
    let history = grains(&request, 1);
    assert_eq!(history[1], vec![vec![0., 1.], vec![1., 0.]]);
    assert_eq!(
        serde_json::to_value(&request.automaton.boundary).unwrap(),
        before
    );
}

#[test]
fn contribution_bounds_and_integer_constraints_are_checked_after_aggregation() {
    let mut value: Value = serde_json::to_value(request()).unwrap();
    value["automaton"]["rules"][0]["condition"] = json!({"kind":"always"});
    value["automaton"]["rules"][0]["actions"] = json!([
        {"kind":"add_neighbor_property","offset":[0,1],"property":"grains","value":{"kind":"constant","args":0.5}},
        {"kind":"add_neighbor_property","offset":[0,1],"property":"grains","value":{"kind":"constant","args":-0.5}}
    ]);
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    let history = grains(&request, 1);
    assert_eq!(history[0], history[1]);
}

#[test]
fn missing_destination_properties_and_out_of_range_results_fail_without_mutating_input() {
    let mut value: Value = serde_json::to_value(request()).unwrap();
    value["automaton"]["rules"][0]["condition"] = json!({
        "kind":"all", "args":[
            {"kind":"type_is","args":0},
            value["automaton"]["rules"][0]["condition"].clone()
        ]
    });
    value["automaton"]["object_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":1,"name":"rock","properties":{}}));
    value["initial_world"]["cells"][0][1] = json!({"type_id":1,"properties":{}});
    for missing_property in [true, false] {
        let mut variant = value.clone();
        if !missing_property {
            variant["initial_world"]["cells"][0][1] =
                json!({"type_id":0,"properties":{"grains":0}});
            variant["automaton"]["object_types"][0]["properties"]["grains"]["max_value"] = json!(8);
            variant["automaton"]["rules"][0]["actions"][1]["value"]["args"] = json!(9);
        }
        let request: SimulationRequest = serde_json::from_value(variant).unwrap();
        let before = serde_json::to_value(&request.initial_world).unwrap();
        let error = run(&request.automaton, &request.initial_world, 1, 42, true).unwrap_err();
        assert!(
            error.contains("Generation 1") && error.contains("Cell (0, 1)"),
            "{error}"
        );
        assert!(
            error.contains(if missing_property {
                "Missing neighbor contribution property"
            } else {
                "Neighbor contributions"
            }),
            "{error}"
        );
        assert_eq!(
            serde_json::to_value(&request.initial_world).unwrap(),
            before
        );
    }
}

#[test]
fn rejects_non_finite_accumulation() {
    let mut value: Value = serde_json::to_value(request()).unwrap();
    value["automaton"]["rules"][0]["actions"] = json!([
        {"kind":"add_neighbor_property","offset":[0,1],"property":"grains","value":{"kind":"constant","args":1e308}},
        {"kind":"add_neighbor_property","offset":[0,1],"property":"grains","value":{"kind":"constant","args":1e308}}
    ]);
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    let error = run(&request.automaton, &request.initial_world, 1, 42, true).unwrap_err();
    assert!(error.contains("contribution sum must be finite"), "{error}");
}

#[test]
fn transfer_references_and_target_are_validated_even_for_unused_rules_and_zero_steps() {
    let base = serde_json::to_value(request()).unwrap();
    for (field, invalid, expected) in [
        ("offset", json!([2, 0]), "not included in the neighborhood"),
        ("property", json!("missing"), "Unknown property"),
        (
            "value",
            json!({"kind":"constant","args":1}),
            "only allowed in cell rules",
        ),
    ] {
        let mut value = base.clone();
        value["automaton"]["rules"][0]["probability"] = json!(0);
        value["automaton"]["rules"][0]["actions"][1][field] = invalid;
        if field == "value" {
            value["automaton"]["rules"][0]["target"] = json!("agent");
        }
        let request: SimulationRequest = serde_json::from_value(value).unwrap();
        let error = run(&request.automaton, &request.initial_world, 0, 42, true).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn transfer_expressions_participate_in_request_depth_limits() {
    let mut value = serde_json::to_value(request()).unwrap();
    let mut expression = json!({"kind":"property","args":"grains"});
    for _ in 0..18 {
        expression = json!({"kind":"add","args":[expression,{"kind":"constant","args":0}]});
    }
    value["automaton"]["rules"][0]["actions"][1]["value"] = expression;
    let request: SimulationRequest = serde_json::from_value(value).unwrap();
    assert!(
        request
            .validate_limits()
            .unwrap_err()
            .contains("nesting exceeds depth")
    );
}
