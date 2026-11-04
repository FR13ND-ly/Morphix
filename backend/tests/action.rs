use std::collections::BTreeMap;

use backend::models::{
    action::{Action, NeighborSelection, apply_actions},
    agent::Agent,
    automaton::{AutomatonDefinition, Boundary},
    condition::{Comparison, Condition},
    expression::Expression,
    neighborhood::{Neighborhood, Neighbors},
    object_type::{ObjectType, PropertyDefinition},
    position::Position,
    state::ObjectState,
};
use rand::{SeedableRng, rngs::StdRng};

#[test]
fn selects_one_neighbor_agent_by_type_and_returns_its_id_without_mutating_it() {
    let definition = automaton();
    let current = rabbit();
    let other_type = Agent {
        id: 5,
        position: Position { row: 0, col: 1 },
        state: definition.object_types[0].create_state().unwrap(),
    };
    let first = Agent {
        id: 20,
        position: Position { row: 0, col: 1 },
        state: rabbit(),
    };
    let second = Agent {
        id: 10,
        position: Position { row: 0, col: 2 },
        state: rabbit(),
    };
    let neighbors = Neighbors {
        agents: vec![&other_type, &first, &second],
        ..Neighbors::default()
    };
    let actions = [
        set(
            "energy",
            Expression::Add(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(10.0)),
            ),
        ),
        Action::RemoveNeighborAgent {
            type_id: 1,
            selection: NeighborSelection::First,
        },
    ];
    let result = apply_actions(
        &actions,
        &current,
        &current,
        &neighbors,
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(result.neighbor_to_remove, Some(20));
    assert_eq!(result.state.properties["energy"], 22.0);
    assert!(!result.remove_agent);
    assert_eq!(first.state.properties["energy"], 12.0);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn random_agent_selection_replays_and_treats_agents_at_one_position_individually() {
    let definition = automaton();
    let current = rabbit();
    let first = Agent {
        id: 20,
        position: Position { row: 0, col: 1 },
        state: rabbit(),
    };
    let second = Agent {
        id: 10,
        position: first.position,
        state: rabbit(),
    };
    let neighbors = Neighbors {
        agents: vec![&first, &second],
        ..Neighbors::default()
    };
    let actions = [Action::RemoveNeighborAgent {
        type_id: 1,
        selection: NeighborSelection::Random,
    }];
    let mut rng = StdRng::seed_from_u64(42);
    let mut replay_rng = StdRng::seed_from_u64(42);
    let mut chosen = Vec::new();
    for _ in 0..64 {
        let result = apply_actions(
            &actions,
            &current,
            &current,
            &neighbors,
            &definition,
            &mut rng,
        )
        .unwrap();
        let replay = apply_actions(
            &actions,
            &current,
            &current,
            &neighbors,
            &definition,
            &mut replay_rng,
        )
        .unwrap();
        assert_eq!(result.neighbor_to_remove, replay.neighbor_to_remove);
        assert!(matches!(result.neighbor_to_remove, Some(20) | Some(10)));
        chosen.push(result.neighbor_to_remove);
    }
    assert!(chosen.contains(&Some(20)));
    assert!(chosen.contains(&Some(10)));
}

#[test]
fn no_neighbor_target_skips_the_whole_batch_before_evaluating_other_actions() {
    let definition = automaton();
    let current = rabbit();
    let zero = Expression::Subtract(
        Box::new(Expression::Property("age".to_owned())),
        Box::new(Expression::Property("age".to_owned())),
    );
    let actions = [
        set(
            "energy",
            Expression::Divide(Box::new(Expression::Constant(1.0)), Box::new(zero)),
        ),
        Action::Move {
            row_offset: Expression::Constant(0.0),
            col_offset: Expression::Constant(1.0),
        },
        spawn_agent(BTreeMap::new()),
        Action::RemoveNeighborAgent {
            type_id: 1,
            selection: NeighborSelection::First,
        },
    ];
    let result = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(result.state.properties, current.properties);
    assert_eq!(result.neighbor_to_remove, None);
    assert_eq!(result.movement, None);
    assert!(result.spawn.is_none());
    assert!(result.cell_update.is_none());
}

#[test]
fn rejects_repeated_neighbor_removals_and_unknown_types_even_without_candidates() {
    let definition = automaton();
    let current = rabbit();
    for (actions, message) in [
        (
            vec![
                Action::RemoveNeighborAgent {
                    type_id: 1,
                    selection: NeighborSelection::First
                };
                2
            ],
            "At most one RemoveNeighborAgent",
        ),
        (
            vec![Action::RemoveNeighborAgent {
                type_id: 99,
                selection: NeighborSelection::First,
            }],
            "Unknown object type id 99",
        ),
    ] {
        let error = apply_actions(
            &actions,
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn chooses_the_first_free_neighbor_matching_the_optional_cell_type() {
    let mut definition = automaton();
    definition.neighborhood.offsets = vec![(0, 1), (0, -1)];
    let current = rabbit();
    let grass = definition.object_types[0].create_state().unwrap();
    let other = rabbit();
    let neighbors = Neighbors {
        offsets: vec![(0, 1), (0, -1)],
        free_offsets: vec![(0, 1), (0, -1)],
        cells: vec![&other, &grass],
        ..Neighbors::default()
    };
    for (cell_type, expected) in [(None, (0, 1)), (Some(0), (0, -1)), (Some(1), (0, 1))] {
        let action = Action::MoveToFreeNeighbor {
            selection: NeighborSelection::First,
            cell_type,
            offsets: None,
        };
        let result = apply_actions(
            &[action],
            &current,
            &grass,
            &neighbors,
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap();
        assert_eq!(result.movement, Some(expected));
        assert_eq!(result.state.properties, current.properties);
    }
}

#[test]
fn random_free_neighbor_choices_replay_with_the_same_rng_seed() {
    let mut definition = automaton();
    definition.neighborhood.offsets = vec![(0, -1), (0, 1)];
    let current = rabbit();
    let grass = definition.object_types[0].create_state().unwrap();
    let neighbors = Neighbors {
        offsets: vec![(0, -1), (0, 1)],
        free_offsets: vec![(0, -1), (0, 1)],
        cells: vec![&grass, &grass],
        ..Neighbors::default()
    };
    let actions = [Action::MoveToFreeNeighbor {
        selection: NeighborSelection::Random,
        cell_type: None,
        offsets: None,
    }];
    let mut rng = StdRng::seed_from_u64(42);
    let mut replay_rng = StdRng::seed_from_u64(42);
    let mut choices = Vec::new();
    for _ in 0..64 {
        let result = apply_actions(
            &actions,
            &current,
            &grass,
            &neighbors,
            &definition,
            &mut rng,
        )
        .unwrap();
        let replay = apply_actions(
            &actions,
            &current,
            &grass,
            &neighbors,
            &definition,
            &mut replay_rng,
        )
        .unwrap();
        assert_eq!(result.movement, replay.movement);
        assert!(matches!(result.movement, Some((0, -1)) | Some((0, 1))));
        choices.push(result.movement);
    }
    assert!(choices.contains(&Some((0, -1))));
    assert!(choices.contains(&Some((0, 1))));
}

#[test]
fn no_eligible_free_neighbor_keeps_other_changes_and_rejects_unknown_type_filters() {
    let definition = automaton();
    let current = rabbit();
    for selection in [NeighborSelection::First, NeighborSelection::Random] {
        let actions = [
            set(
                "energy",
                Expression::Subtract(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(1.0)),
                ),
            ),
            Action::MoveToFreeNeighbor {
                selection,
                cell_type: Some(0),
                offsets: None,
            },
        ];
        let result = apply_actions(
            &actions,
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap();
        assert_eq!(result.movement, None);
        assert_eq!(result.state.properties["energy"], 11.0);
        let invalid = Action::MoveToFreeNeighbor {
            selection,
            cell_type: Some(99),
            offsets: None,
        };
        assert!(invalid.validate(&definition).is_err());
        let error = apply_actions(
            &[invalid],
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap_err();
        assert!(error.contains("Unknown object type id 99"), "{error}");
    }
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn rejects_mixed_movement_actions_even_when_no_free_neighbor_can_be_selected() {
    let definition = automaton();
    let current = rabbit();
    let free = Action::MoveToFreeNeighbor {
        selection: NeighborSelection::First,
        cell_type: None,
        offsets: None,
    };
    let explicit = Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
    };
    for actions in [
        vec![free.clone(), explicit.clone()],
        vec![explicit, free.clone()],
        vec![free.clone(), free],
    ] {
        let error = apply_actions(
            &actions,
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap_err();
        assert!(error.contains("At most one movement action"), "{error}");
    }
}

fn property(default_value: f64, integer_only: bool) -> PropertyDefinition {
    PropertyDefinition {
        default_value,
        min_value: 0.0,
        max_value: 100.0,
        integer_only,
    }
}

fn automaton() -> AutomatonDefinition {
    AutomatonDefinition {
        name: "Ecosystem".to_owned(),
        object_types: vec![
            ObjectType {
                id: 0,
                name: "grass".to_owned(),
                properties: BTreeMap::from([("value".to_owned(), property(10.0, false))]),
            },
            ObjectType {
                id: 1,
                name: "rabbit".to_owned(),
                properties: BTreeMap::from([
                    ("energy".to_owned(), property(10.0, false)),
                    ("age".to_owned(), property(0.0, true)),
                ]),
            },
        ],
        neighborhood: Neighborhood {
            offsets: Vec::new(),
        },
        boundary: Boundary::Wrap,
        rules: Vec::new(),
    }
}

fn rabbit() -> ObjectState {
    ObjectState {
        type_id: 1,
        properties: BTreeMap::from([("energy".to_owned(), 12.0), ("age".to_owned(), 3.0)]),
    }
}

fn set(property: &str, value: Expression) -> Action {
    Action::SetProperty {
        property: property.to_owned(),
        value,
    }
}

#[test]
fn updates_a_property_using_an_expression_without_changing_the_input() {
    let automaton = automaton();
    let current = rabbit();
    let actions = [set(
        "energy",
        Expression::Subtract(
            Box::new(Expression::Property("energy".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    )];

    let next = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.state.properties["energy"], 11.0);
    assert_eq!(next.state.properties["age"], 3.0);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn reads_original_values_for_all_actions_and_keeps_the_last_assignment() {
    let automaton = automaton();
    let current = rabbit();
    let actions = [
        set("energy", Expression::Constant(13.0)),
        set("age", Expression::Property("energy".to_owned())),
        set("energy", Expression::Constant(14.0)),
    ];

    let next = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.state.properties["energy"], 14.0);
    assert_eq!(next.state.properties["age"], 12.0);
}

#[test]
fn changing_type_replaces_properties_with_the_target_types_defaults() {
    let automaton = automaton();
    let current = rabbit();
    let next = apply_actions(
        &[Action::SetType { type_id: 0 }],
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.state.type_id, 0);
    assert_eq!(
        next.state.properties,
        BTreeMap::from([("value".to_owned(), 10.0)])
    );
    assert_eq!(current.type_id, 1);
}

#[test]
fn can_override_new_types_defaults_using_the_original_objects_properties() {
    let automaton = automaton();
    let current = ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("value".to_owned(), 20.0)]),
    };
    let actions = [
        Action::SetType { type_id: 1 },
        set(
            "energy",
            Expression::Add(
                Box::new(Expression::Property("value".to_owned())),
                Box::new(Expression::Constant(2.0)),
            ),
        ),
    ];

    let next = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.state.type_id, 1);
    assert_eq!(next.state.properties["energy"], 22.0);
    assert_eq!(next.state.properties["age"], 0.0);
    assert!(!next.state.properties.contains_key("value"));
}

#[test]
fn rejects_invalid_action_batches_without_partially_changing_the_input() {
    let automaton = automaton();
    let current = rabbit();

    for invalid in [
        set("energy", Expression::Constant(101.0)),
        set("age", Expression::Constant(2.5)),
        set("unknown", Expression::Constant(1.0)),
        Action::SetType { type_id: 99 },
        set(
            "energy",
            Expression::Divide(
                Box::new(Expression::Constant(1.0)),
                Box::new(Expression::Constant(0.0)),
            ),
        ),
    ] {
        let actions = [set("energy", Expression::Constant(13.0)), invalid];

        assert!(
            apply_actions(
                &actions,
                &current,
                &current,
                &Neighbors::default(),
                &automaton,
                &mut StdRng::seed_from_u64(42),
            )
            .is_err()
        );
        assert_eq!(current.properties["energy"], 12.0);
        assert_eq!(current.properties["age"], 3.0);
    }
}

#[test]
fn validates_the_final_state_after_all_assignments() {
    let automaton = automaton();
    let current = rabbit();
    let actions = [
        set("energy", Expression::Constant(-1.0)),
        set("energy", Expression::Constant(5.0)),
    ];

    let next = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(next.state.properties["energy"], 5.0);
}

#[test]
fn creates_default_states_and_rejects_invalid_defaults() {
    let automaton = automaton();
    let rabbit_type = &automaton.object_types[1];
    let created = rabbit_type.create_state().unwrap();

    assert_eq!(created.type_id, 1);
    assert_eq!(created.properties["energy"], 10.0);
    assert_eq!(created.properties["age"], 0.0);
    assert!(rabbit_type.validate_state(&created).is_ok());

    let mut invalid_type = rabbit_type.clone();
    invalid_type
        .properties
        .get_mut("age")
        .unwrap()
        .default_value = 0.5;
    assert!(invalid_type.create_state().is_err());
}

#[test]
fn validates_input_and_allows_an_empty_action_list() {
    let automaton = automaton();
    let current = rabbit();
    let next = apply_actions(
        &[],
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.state.type_id, current.type_id);
    assert_eq!(next.state.properties, current.properties);
    assert!(!next.remove_agent);
    assert_eq!(next.movement, None);
    assert!(next.spawn.is_none());
    assert!(next.cell_update.is_none());

    let mut invalid = rabbit();
    invalid.properties.clear();
    assert!(
        apply_actions(
            &[],
            &invalid,
            &invalid,
            &Neighbors::default(),
            &automaton,
            &mut StdRng::seed_from_u64(42),
        )
        .is_err()
    );
}

#[test]
fn combines_a_condition_with_an_expression_based_action() {
    let automaton = automaton();
    let current = rabbit();
    let condition = Condition::Compare {
        left: Expression::Property("energy".to_owned()),
        operator: Comparison::Greater,
        right: Expression::Constant(0.0),
    };
    let actions = [set(
        "energy",
        Expression::Subtract(
            Box::new(Expression::Property("energy".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    )];

    assert!(
        condition
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );
    let next = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(next.state.properties["energy"], 11.0);
}

#[test]
fn removal_returns_an_intent_without_modifying_the_original_state() {
    let automaton = automaton();
    let current = rabbit();

    let result = apply_actions(
        &[Action::RemoveAgent {}],
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert!(result.remove_agent);
    assert_eq!(result.movement, None);
    assert_eq!(result.state.type_id, current.type_id);
    assert_eq!(result.state.properties, current.properties);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn rejects_removal_combined_with_other_actions_in_either_order() {
    let automaton = automaton();
    let current = rabbit();

    for other in [
        Action::RemoveAgent {},
        Action::SetType { type_id: 0 },
        set("energy", Expression::Constant(0.0)),
        Action::Move {
            row_offset: Expression::Constant(0.0),
            col_offset: Expression::Constant(1.0),
        },
        Action::MoveToFreeNeighbor {
            selection: NeighborSelection::Random,
            cell_type: None,
            offsets: None,
        },
        Action::RemoveNeighborAgent {
            type_id: 1,
            selection: NeighborSelection::First,
        },
        spawn_agent(BTreeMap::new()),
        Action::SetCellProperty {
            property: "value".to_owned(),
            value: Expression::Constant(0.0),
        },
        Action::SetCellType { type_id: 0 },
    ] {
        for actions in [
            vec![Action::RemoveAgent {}, other.clone()],
            vec![other.clone(), Action::RemoveAgent {}],
        ] {
            let error = apply_actions(
                &actions,
                &current,
                &current,
                &Neighbors::default(),
                &automaton,
                &mut StdRng::seed_from_u64(42),
            )
            .unwrap_err();
            assert!(
                error.contains("RemoveAgent must be the only action"),
                "{error}"
            );
            assert_eq!(current.properties["energy"], 12.0);
        }
    }
}

fn spawn_agent(properties: BTreeMap<String, Expression>) -> Action {
    Action::SpawnAgent {
        type_id: 1,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
        properties,
    }
}

#[test]
fn spawning_uses_the_new_types_defaults_and_keeps_the_source_state() {
    let definition = automaton();
    let current = rabbit();

    let result = apply_actions(
        &[spawn_agent(BTreeMap::new())],
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.properties, current.properties);
    assert!(!result.remove_agent);
    assert_eq!(result.movement, None);
    let spawn = result.spawn.unwrap();
    assert_eq!(spawn.offset, (0, 1));
    assert_eq!(spawn.state.type_id, 1);
    assert_eq!(spawn.state.properties["energy"], 10.0);
    assert_eq!(spawn.state.properties["age"], 0.0);
}

#[test]
fn spawn_expressions_read_the_original_source_even_after_it_changes_type() {
    let definition = automaton();
    let current = rabbit();
    let actions = [
        Action::SetType { type_id: 0 },
        set("value", Expression::Constant(4.0)),
        Action::SpawnAgent {
            type_id: 1,
            row_offset: Expression::Property("age".to_owned()),
            col_offset: Expression::Constant(-1.0),
            properties: BTreeMap::from([(
                "energy".to_owned(),
                Expression::Divide(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(2.0)),
                ),
            )]),
        },
    ];

    let result = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.type_id, 0);
    assert_eq!(result.state.properties["value"], 4.0);
    let spawn = result.spawn.unwrap();
    assert_eq!(spawn.offset, (3, -1));
    assert_eq!(spawn.state.type_id, 1);
    assert_eq!(spawn.state.properties["energy"], 6.0);
    assert_eq!(spawn.state.properties["age"], 0.0);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn rejects_invalid_spawn_states_and_offsets_without_changing_the_source() {
    let definition = automaton();
    let current = rabbit();
    let mut fractional_offset = spawn_agent(BTreeMap::new());
    if let Action::SpawnAgent { row_offset, .. } = &mut fractional_offset {
        *row_offset = Expression::Divide(
            Box::new(Expression::Property("age".to_owned())),
            Box::new(Expression::Constant(2.0)),
        );
    }

    for invalid in [
        spawn_agent(BTreeMap::from([(
            "unknown".to_owned(),
            Expression::Constant(1.0),
        )])),
        spawn_agent(BTreeMap::from([(
            "energy".to_owned(),
            Expression::Multiply(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(10.0)),
            ),
        )])),
        spawn_agent(BTreeMap::from([(
            "age".to_owned(),
            Expression::Constant(1.5),
        )])),
        spawn_agent(BTreeMap::from([(
            "energy".to_owned(),
            Expression::Property("missing".to_owned()),
        )])),
        fractional_offset,
    ] {
        assert!(
            apply_actions(
                &[invalid],
                &current,
                &current,
                &Neighbors::default(),
                &definition,
                &mut StdRng::seed_from_u64(42),
            )
            .is_err()
        );
        assert_eq!(current.properties["energy"], 12.0);
        assert_eq!(current.properties["age"], 3.0);
    }
}

#[test]
fn direct_action_batches_also_reject_multiple_spawns() {
    let definition = automaton();
    let current = rabbit();
    let actions = [spawn_agent(BTreeMap::new()), spawn_agent(BTreeMap::new())];

    let error = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap_err();

    assert!(error.contains("At most one SpawnAgent"), "{error}");
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn actions_read_the_original_cell_for_state_movement_and_spawn_expressions() {
    let definition = automaton();
    let current = rabbit();
    let cell = ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("value".to_owned(), 4.0)]),
    };
    let actions = [
        Action::SetType { type_id: 0 },
        set(
            "value",
            Expression::Add(
                Box::new(Expression::CellProperty("value".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
        Action::Move {
            row_offset: Expression::Constant(0.0),
            col_offset: Expression::CellProperty("value".to_owned()),
        },
        Action::SpawnAgent {
            type_id: 1,
            row_offset: Expression::Constant(0.0),
            col_offset: Expression::CellProperty("value".to_owned()),
            properties: BTreeMap::from([(
                "energy".to_owned(),
                Expression::Add(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::CellProperty("value".to_owned())),
                ),
            )]),
        },
    ];

    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.type_id, 0);
    assert_eq!(result.state.properties["value"], 5.0);
    assert_eq!(result.movement, Some((0, 4)));
    let spawn = result.spawn.unwrap();
    assert_eq!(spawn.offset, (0, 4));
    assert_eq!(spawn.state.properties["energy"], 16.0);
    assert_eq!(cell.properties["value"], 4.0);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn cell_rules_keep_reading_the_old_cell_after_earlier_assignments() {
    let definition = automaton();
    let current = ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("value".to_owned(), 4.0)]),
    };
    let actions = [
        set("value", Expression::Constant(8.0)),
        set(
            "value",
            Expression::Add(
                Box::new(Expression::CellProperty("value".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
    ];

    let result = apply_actions(
        &actions,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.properties["value"], 5.0);
    assert_eq!(current.properties["value"], 4.0);
}

fn grass() -> ObjectState {
    ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("value".to_owned(), 10.0)]),
    }
}

fn set_cell(property: &str, value: Expression) -> Action {
    Action::SetCellProperty {
        property: property.to_owned(),
        value,
    }
}

#[test]
fn calculates_energy_and_food_changes_without_mutating_either_input() {
    let definition = automaton();
    let current = rabbit();
    let cell = grass();
    let actions = [
        set(
            "energy",
            Expression::Add(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::CellProperty("value".to_owned())),
            ),
        ),
        set_cell("value", Expression::Constant(0.0)),
    ];

    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.properties["energy"], 22.0);
    assert_eq!(result.state.properties["age"], 3.0);
    let update = result.cell_update.unwrap();
    assert_eq!(update.type_id, None);
    assert_eq!(
        update.properties,
        BTreeMap::from([("value".to_owned(), 0.0)])
    );
    assert_eq!(
        update.apply(&cell, &definition).unwrap().properties["value"],
        0.0
    );
    assert_eq!(cell.properties["value"], 10.0);
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn a_cell_property_update_preserves_other_properties_of_the_evolved_cell() {
    let mut definition = automaton();
    definition.object_types[0]
        .properties
        .insert("temperature".to_owned(), property(2.0, false));
    let cell = definition.object_types[0].create_state().unwrap();
    let result = apply_actions(
        &[set_cell("value", Expression::Constant(0.0))],
        &rabbit(),
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    let mut evolved_cell = cell.clone();
    evolved_cell.properties.insert("value".to_owned(), 15.0);
    evolved_cell
        .properties
        .insert("temperature".to_owned(), 7.0);

    let updated = result
        .cell_update
        .unwrap()
        .apply(&evolved_cell, &definition)
        .unwrap();

    assert_eq!(updated.properties["value"], 0.0);
    assert_eq!(updated.properties["temperature"], 7.0);
    assert_eq!(cell.properties["temperature"], 2.0);
}

#[test]
fn cell_type_changes_reset_earlier_cell_assignments_and_use_new_defaults() {
    let definition = automaton();
    let current = rabbit();
    let cell = grass();
    let actions = [
        set_cell("value", Expression::Constant(0.0)),
        Action::SetCellType { type_id: 1 },
        set_cell(
            "energy",
            Expression::Multiply(
                Box::new(Expression::CellProperty("value".to_owned())),
                Box::new(Expression::Constant(2.0)),
            ),
        ),
    ];

    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    let update = result.cell_update.unwrap();
    let updated = update.apply(&cell, &definition).unwrap();

    assert_eq!(update.type_id, Some(1));
    assert!(!update.properties.contains_key("value"));
    assert_eq!(updated.type_id, 1);
    assert_eq!(updated.properties["energy"], 20.0);
    assert_eq!(updated.properties["age"], 0.0);
    assert_eq!(result.state.properties, current.properties);
    assert_eq!(cell.type_id, 0);
}

#[test]
fn cell_assignments_read_the_original_cell_and_validate_the_final_batch() {
    let definition = automaton();
    let current = rabbit();
    let cell = grass();
    let actions = [
        set_cell("value", Expression::Constant(1.0)),
        set_cell(
            "value",
            Expression::Subtract(
                Box::new(Expression::CellProperty("value".to_owned())),
                Box::new(Expression::Constant(2.0)),
            ),
        ),
    ];
    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(
        result
            .cell_update
            .unwrap()
            .apply(&cell, &definition)
            .unwrap()
            .properties["value"],
        8.0
    );

    let actions = [
        set_cell("value", Expression::Constant(-1.0)),
        set_cell("value", Expression::Constant(0.0)),
    ];
    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();
    assert_eq!(
        result
            .cell_update
            .unwrap()
            .apply(&cell, &definition)
            .unwrap()
            .properties["value"],
        0.0
    );
}

#[test]
fn rejects_invalid_cell_updates_without_modifying_inputs() {
    let definition = automaton();
    let current = rabbit();
    let cell = grass();

    for actions in [
        vec![set_cell("unknown", Expression::Constant(0.0))],
        vec![
            set_cell("energy", Expression::Constant(1.0)),
            Action::SetCellType { type_id: 1 },
        ],
        vec![set_cell("value", Expression::Constant(101.0))],
        vec![Action::SetCellType { type_id: 99 }],
        vec![
            Action::SetCellType { type_id: 1 },
            set_cell("age", Expression::Constant(0.5)),
        ],
        vec![set_cell(
            "value",
            Expression::Divide(
                Box::new(Expression::Constant(1.0)),
                Box::new(Expression::Constant(0.0)),
            ),
        )],
    ] {
        assert!(
            apply_actions(
                &actions,
                &current,
                &cell,
                &Neighbors::default(),
                &definition,
                &mut StdRng::seed_from_u64(42),
            )
            .is_err()
        );
        assert_eq!(cell.properties["value"], 10.0);
        assert_eq!(current.properties["energy"], 12.0);
    }
}

#[test]
fn agent_observations_reach_state_movement_spawn_and_cell_update_expressions() {
    let definition = automaton();
    let current = rabbit();
    let cell = grass();
    let mut first = Agent {
        id: 7,
        position: Position { row: 0, col: 1 },
        state: rabbit(),
    };
    first.state.properties.insert("energy".to_owned(), 4.0);
    let mut second = first.clone();
    second.id = 8;
    second.state.properties.insert("energy".to_owned(), 6.0);
    let neighbors = Neighbors {
        agents: vec![&first, &second],
        ..Neighbors::default()
    };
    let count = Expression::CountNeighborAgents { type_id: 1 };
    let sum = Expression::SumNeighborAgents {
        type_id: 1,
        property: "energy".to_owned(),
    };
    let actions = [
        set(
            "energy",
            Expression::Add(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(sum.clone()),
            ),
        ),
        Action::Move {
            row_offset: Expression::Constant(0.0),
            col_offset: count.clone(),
        },
        Action::SpawnAgent {
            type_id: 1,
            row_offset: Expression::Constant(0.0),
            col_offset: count.clone(),
            properties: BTreeMap::from([("energy".to_owned(), sum)]),
        },
        set_cell("value", count),
    ];

    let result = apply_actions(
        &actions,
        &current,
        &cell,
        &neighbors,
        &definition,
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(result.state.properties["energy"], 22.0);
    assert_eq!(result.movement, Some((0, 2)));
    let spawn = result.spawn.unwrap();
    assert_eq!(spawn.offset, (0, 2));
    assert_eq!(spawn.state.properties["energy"], 10.0);
    assert_eq!(
        result
            .cell_update
            .unwrap()
            .apply(&cell, &definition)
            .unwrap()
            .properties["value"],
        2.0
    );
    assert_eq!(current.properties["energy"], 12.0);
    assert_eq!(cell.properties["value"], 10.0);
}
