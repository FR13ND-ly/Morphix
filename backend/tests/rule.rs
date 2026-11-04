use std::collections::BTreeMap;

use backend::models::{
    action::{Action, NeighborSelection},
    agent::Agent,
    automaton::{AutomatonDefinition, Boundary},
    condition::{Comparison, Condition},
    expression::Expression,
    neighborhood::{Neighborhood, Neighbors},
    object_type::{ObjectType, PropertyDefinition},
    position::Position,
    rule::{Rule, RuleTarget, apply_rules},
    state::ObjectState,
    world::World,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

#[test]
fn validates_neighbor_removal_targets_types_and_action_counts_in_unused_rules() {
    let removal = Action::RemoveNeighborAgent {
        type_id: 1,
        selection: NeighborSelection::First,
    };
    for (target, actions, message) in [
        (
            RuleTarget::Cell,
            vec![removal.clone()],
            "RemoveNeighborAgent is only allowed in agent rules",
        ),
        (
            RuleTarget::Agent,
            vec![removal.clone(), removal.clone()],
            "at most one RemoveNeighborAgent",
        ),
        (
            RuleTarget::Agent,
            vec![removal, Action::RemoveAgent {}],
            "RemoveAgent must be the only action",
        ),
        (
            RuleTarget::Agent,
            vec![Action::RemoveNeighborAgent {
                type_id: 99,
                selection: NeighborSelection::Random,
            }],
            "Unknown object type id 99",
        ),
    ] {
        let definition = automaton(vec![Rule {
            target,
            condition: Condition::Any(Vec::new()),
            probability: Expression::Constant(0.0),
            actions,
        }]);
        let error = definition.validate().unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    let valid = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::RemoveNeighborAgent {
                type_id: 1,
                selection: NeighborSelection::Random,
            },
            set_energy(Expression::Constant(20.0)),
        ],
    }]);
    valid.validate().unwrap();
}

#[test]
fn validates_free_neighbor_movement_target_filter_and_mixed_movement_count() {
    let free = Action::MoveToFreeNeighbor {
        selection: NeighborSelection::Random,
        cell_type: Some(0),
        offsets: None,
    };
    let explicit = Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
    };
    let cases = [
        (
            RuleTarget::Cell,
            vec![free.clone()],
            "MoveToFreeNeighbor is only allowed in agent rules",
        ),
        (
            RuleTarget::Agent,
            vec![free.clone(), explicit.clone()],
            "at most one movement action",
        ),
        (
            RuleTarget::Agent,
            vec![explicit, free.clone()],
            "at most one movement action",
        ),
        (
            RuleTarget::Agent,
            vec![free.clone(), free],
            "at most one movement action",
        ),
        (
            RuleTarget::Agent,
            vec![Action::MoveToFreeNeighbor {
                selection: NeighborSelection::First,
                cell_type: Some(99),
                offsets: None,
            }],
            "Unknown object type id 99",
        ),
    ];
    for (target, actions, message) in cases {
        let definition = automaton(vec![Rule {
            target,
            condition: Condition::Any(Vec::new()),
            probability: Expression::Constant(0.0),
            actions,
        }]);
        let error = definition.validate().unwrap_err();
        assert!(error.contains(message), "{error}");
    }
    let valid = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![Action::MoveToFreeNeighbor {
            selection: NeighborSelection::First,
            cell_type: None,
            offsets: None,
        }],
    }]);
    valid.validate().unwrap();
}

fn automaton(rules: Vec<Rule>) -> AutomatonDefinition {
    AutomatonDefinition {
        name: "Example".to_owned(),
        object_types: vec![
            ObjectType {
                id: 0,
                name: "dead".to_owned(),
                properties: BTreeMap::new(),
            },
            ObjectType {
                id: 1,
                name: "alive".to_owned(),
                properties: BTreeMap::new(),
            },
            ObjectType {
                id: 2,
                name: "rabbit".to_owned(),
                properties: BTreeMap::from([(
                    "energy".to_owned(),
                    PropertyDefinition {
                        default_value: 10.0,
                        min_value: 0.0,
                        max_value: 100.0,
                        integer_only: false,
                    },
                )]),
            },
        ],
        neighborhood: Neighborhood {
            offsets: vec![
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ],
        },
        boundary: Boundary::Fixed { state: state(0) },
        rules,
    }
}

fn state(type_id: u8) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::new(),
    }
}

#[test]
fn validates_directional_references_even_in_unused_condition_branches_and_actions() {
    for (condition, expression, message) in [
        (
            Condition::NeighborTypeIs {
                offset: (0, 2),
                type_id: 0,
            },
            Expression::Constant(1.0),
            "offset (0, 2)",
        ),
        (
            Condition::NeighborTypeIs {
                offset: (0, 1),
                type_id: 99,
            },
            Expression::Constant(1.0),
            "Unknown object type id 99",
        ),
        (
            Condition::Always,
            Expression::NeighborProperty {
                offset: (0, 2),
                property: "energy".to_owned(),
            },
            "offset (0, 2)",
        ),
        (
            Condition::Always,
            Expression::NeighborProperty {
                offset: (0, 1),
                property: "missing".to_owned(),
            },
            "Unknown property 'missing'",
        ),
    ] {
        let definition = automaton(vec![Rule {
            target: RuleTarget::Agent,
            condition: Condition::Any(vec![Condition::Always, condition]),
            probability: Expression::Constant(0.0),
            actions: vec![Action::SetProperty {
                property: "energy".to_owned(),
                value: expression,
            }],
        }]);
        let error = definition.validate().unwrap_err();
        assert!(error.contains("Rule 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

fn change_type(target: RuleTarget, condition: Condition, probability: f64, type_id: u8) -> Rule {
    Rule {
        target,
        condition,
        probability: Expression::Constant(probability),
        actions: vec![Action::SetType { type_id }],
    }
}

#[test]
fn keeps_the_object_when_there_are_no_rules_or_no_matching_conditions() {
    let current = state(0);
    let mut rng = StdRng::seed_from_u64(7);

    for rules in [
        Vec::new(),
        vec![change_type(RuleTarget::Cell, Condition::TypeIs(1), 1.0, 1)],
    ] {
        let automaton = automaton(rules);
        let next = apply_rules(
            RuleTarget::Cell,
            &current,
            &current,
            &Neighbors::default(),
            &automaton,
            &mut rng,
        )
        .unwrap();

        assert_eq!(next.state.type_id, 0);
        assert_eq!(next.state.properties, current.properties);
    }
}

#[test]
fn applies_the_first_matching_rule_and_does_not_chain_changes() {
    let automaton = automaton(vec![
        change_type(RuleTarget::Cell, Condition::TypeIs(1), 1.0, 0),
        change_type(RuleTarget::Cell, Condition::TypeIs(0), 1.0, 1),
        change_type(RuleTarget::Cell, Condition::TypeIs(1), 1.0, 0),
        change_type(RuleTarget::Cell, Condition::Always, 1.0, 0),
    ]);
    let current = state(0);
    let mut rng = StdRng::seed_from_u64(7);

    let next = apply_rules(
        RuleTarget::Cell,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next.state.type_id, 1);
    assert_eq!(current.type_id, 0);
}

#[test]
fn separates_cell_rules_from_agent_rules_before_evaluating_conditions() {
    let mut agent_rule = change_type(RuleTarget::Agent, Condition::Always, 1.0, 0);
    agent_rule.condition = Condition::Compare {
        left: Expression::Property("energy".to_owned()),
        operator: Comparison::Greater,
        right: Expression::Constant(0.0),
    };
    let automaton = automaton(vec![
        agent_rule,
        change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
    ]);
    let current = state(0);
    let rabbit = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let mut rng = StdRng::seed_from_u64(7);

    let next_cell = apply_rules(
        RuleTarget::Cell,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut rng,
    )
    .unwrap();
    let next_agent = apply_rules(
        RuleTarget::Agent,
        &rabbit,
        &rabbit,
        &Neighbors::default(),
        &automaton,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next_cell.state.type_id, 1);
    assert_eq!(next_agent.state.type_id, 0);
}

#[test]
fn a_failed_probability_check_keeps_the_object_and_skips_later_rules() {
    for probability in [0.0, 1.0] {
        let automaton = automaton(vec![
            change_type(RuleTarget::Cell, Condition::Always, probability, 1),
            change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
        ]);
        let current = state(0);
        let mut rng = StdRng::seed_from_u64(7);

        let next = apply_rules(
            RuleTarget::Cell,
            &current,
            &current,
            &Neighbors::default(),
            &automaton,
            &mut rng,
        )
        .unwrap();

        assert_eq!(next.state.type_id, if probability == 1.0 { 1 } else { 0 });
    }
}

#[test]
fn uses_the_probability_and_is_reproducible_with_the_same_seed() {
    let automaton = automaton(vec![change_type(
        RuleTarget::Cell,
        Condition::Always,
        0.25,
        1,
    )]);
    let current = state(0);
    let mut rng = StdRng::seed_from_u64(42);
    let mut replay_rng = StdRng::seed_from_u64(42);
    let mut applied_count = 0;

    for _ in 0..1024 {
        let next = apply_rules(
            RuleTarget::Cell,
            &current,
            &current,
            &Neighbors::default(),
            &automaton,
            &mut rng,
        )
        .unwrap();
        let replay = apply_rules(
            RuleTarget::Cell,
            &current,
            &current,
            &Neighbors::default(),
            &automaton,
            &mut replay_rng,
        )
        .unwrap();

        assert_eq!(next.state.type_id, replay.state.type_id);
        if next.state.type_id == 1 {
            applied_count += 1;
        }
    }

    assert!((192..=320).contains(&applied_count));
}

#[test]
fn rejects_invalid_probabilities_even_in_rules_that_would_be_skipped() {
    let current = state(0);
    let mut rng = StdRng::seed_from_u64(7);

    for probability in [-0.01, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let invalid_rule = change_type(RuleTarget::Agent, Condition::TypeIs(1), probability, 1);
        assert!(invalid_rule.validate(&automaton(Vec::new())).is_err());

        let automaton = automaton(vec![
            change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
            invalid_rule,
        ]);
        let error = automaton.validate().unwrap_err();
        assert!(error.contains("Rule 1"));
        assert!(error.contains("probability"));
        assert!(
            apply_rules(
                RuleTarget::Cell,
                &current,
                &current,
                &Neighbors::default(),
                &automaton,
                &mut rng
            )
            .is_err()
        );
    }
}

#[test]
fn propagates_condition_and_action_errors_without_modifying_the_input() {
    let current = state(0);
    let mut rng = StdRng::seed_from_u64(7);
    let invalid_condition = Rule {
        target: RuleTarget::Cell,
        condition: Condition::Compare {
            left: Expression::Property("missing".to_owned()),
            operator: Comparison::Equal,
            right: Expression::Constant(1.0),
        },
        probability: Expression::Constant(1.0),
        actions: Vec::new(),
    };
    let invalid_action = change_type(RuleTarget::Cell, Condition::Always, 1.0, 99);

    for invalid_rule in [invalid_condition, invalid_action] {
        let automaton = automaton(vec![
            invalid_rule,
            change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
        ]);
        let error = apply_rules(
            RuleTarget::Cell,
            &current,
            &current,
            &Neighbors::default(),
            &automaton,
            &mut rng,
        )
        .unwrap_err();

        assert!(error.contains("Rule 0"));
        assert_eq!(current.type_id, 0);
        assert!(current.properties.is_empty());
    }
}

#[test]
fn validates_the_current_state_before_applying_rules() {
    let automaton = automaton(Vec::new());
    let mut rng = StdRng::seed_from_u64(7);

    for current in [state(99), state(2)] {
        assert!(
            apply_rules(
                RuleTarget::Cell,
                &current,
                &current,
                &Neighbors::default(),
                &automaton,
                &mut rng
            )
            .is_err()
        );
    }
}

#[test]
fn an_empty_action_list_is_a_matching_rule_that_keeps_the_state() {
    let automaton = automaton(vec![
        Rule {
            target: RuleTarget::Cell,
            condition: Condition::Always,
            probability: Expression::Constant(1.0),
            actions: Vec::new(),
        },
        change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
    ]);
    let mut rng = StdRng::seed_from_u64(7);
    let next = apply_rules(
        RuleTarget::Cell,
        &state(0),
        &state(0),
        &Neighbors::default(),
        &automaton,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next.state.type_id, 0);
}

#[test]
fn applies_game_of_life_birth_using_real_neighbors() {
    let birth = Rule {
        target: RuleTarget::Cell,
        condition: Condition::All(vec![
            Condition::TypeIs(0),
            Condition::Compare {
                left: Expression::CountNeighbors { type_id: 1 },
                operator: Comparison::Equal,
                right: Expression::Constant(3.0),
            },
        ]),
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetType { type_id: 1 }],
    };
    let automaton = automaton(vec![birth]);
    let mut rng = StdRng::seed_from_u64(7);

    for (live_neighbors, expected_type) in [(3, 1), (2, 0)] {
        let mut world = World {
            cells: vec![vec![state(0); 3]; 3],
            agents: Vec::new(),
        };
        for col in 0..live_neighbors {
            world.cells[0][col] = state(1);
        }
        automaton.validate_world(&world).unwrap();
        let current = &world.cells[1][1];
        let neighbors = automaton
            .neighborhood
            .read(&world, Position { row: 1, col: 1 }, &automaton.boundary)
            .unwrap();

        let next = apply_rules(
            RuleTarget::Cell,
            current,
            current,
            &neighbors,
            &automaton,
            &mut rng,
        )
        .unwrap();

        assert_eq!(next.state.type_id, expected_type);
        assert_eq!(world.cells[1][1].type_id, 0);
    }
}

#[test]
fn updates_an_agents_energy_using_its_rule() {
    let automaton = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::All(vec![
            Condition::TypeIs(2),
            Condition::Compare {
                left: Expression::Property("energy".to_owned()),
                operator: Comparison::Greater,
                right: Expression::Constant(0.0),
            },
        ]),
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Subtract(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        }],
    }]);
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let mut rng = StdRng::seed_from_u64(7);

    let next = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors::default(),
        &automaton,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next.state.type_id, 2);
    assert_eq!(next.state.properties["energy"], 11.0);
    assert_eq!(current.properties["energy"], 12.0);
}

fn compare_to_zero(expression: Expression) -> Condition {
    Condition::Compare {
        left: expression,
        operator: Comparison::Equal,
        right: Expression::Constant(0.0),
    }
}

fn set_energy(value: Expression) -> Action {
    Action::SetProperty {
        property: "energy".to_owned(),
        value,
    }
}

#[test]
fn rejects_unknown_references_in_all_condition_branches_before_execution() {
    for (condition, message) in [
        (Condition::TypeIs(99), "Unknown object type id 99"),
        (
            compare_to_zero(Expression::CountNeighbors { type_id: 99 }),
            "Unknown object type id 99",
        ),
        (
            compare_to_zero(Expression::Property("missing".to_owned())),
            "Unknown property 'missing'",
        ),
        (
            compare_to_zero(Expression::SumNeighbors {
                property: "missing".to_owned(),
            }),
            "Unknown property 'missing'",
        ),
        (
            Condition::Any(vec![Condition::Always, Condition::TypeIs(99)]),
            "Branch 1: Unknown object type id 99",
        ),
        (
            Condition::All(vec![
                Condition::Not(Box::new(Condition::Always)),
                compare_to_zero(Expression::Property("missing".to_owned())),
            ]),
            "Branch 1: Unknown property 'missing'",
        ),
        (
            Condition::Not(Box::new(Condition::Any(vec![
                Condition::Always,
                Condition::TypeIs(99),
            ]))),
            "Unknown object type id 99",
        ),
    ] {
        let automaton = automaton(vec![change_type(RuleTarget::Agent, condition, 0.0, 1)]);

        let error = automaton.validate().unwrap_err();

        assert!(error.contains("Rule 0: Condition:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn rejects_unknown_action_references_in_unused_rules() {
    for (action, message) in [
        (Action::SetType { type_id: 99 }, "Unknown object type id 99"),
        (
            Action::SetProperty {
                property: "missing".to_owned(),
                value: Expression::Constant(0.0),
            },
            "Unknown property 'missing'",
        ),
        (
            set_energy(Expression::Add(
                Box::new(Expression::Property("missing".to_owned())),
                Box::new(Expression::Constant(1.0)),
            )),
            "Unknown property 'missing'",
        ),
        (
            set_energy(Expression::CountNeighbors { type_id: 99 }),
            "Unknown object type id 99",
        ),
        (
            set_energy(Expression::SumNeighbors {
                property: "missing".to_owned(),
            }),
            "Unknown property 'missing'",
        ),
    ] {
        let automaton = automaton(vec![
            change_type(RuleTarget::Cell, Condition::Always, 1.0, 1),
            Rule {
                target: RuleTarget::Agent,
                condition: Condition::TypeIs(1),
                probability: Expression::Constant(0.0),
                actions: vec![action],
            },
        ]);

        let error = automaton.validate().unwrap_err();

        assert!(error.contains("Rule 1: Action 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn rejects_non_finite_constants_and_invalid_constant_arithmetic() {
    for expression in [
        Expression::Constant(f64::NAN),
        Expression::Constant(f64::INFINITY),
        Expression::Constant(f64::NEG_INFINITY),
        Expression::Add(
            Box::new(Expression::Constant(f64::MAX)),
            Box::new(Expression::Constant(f64::MAX)),
        ),
        Expression::Subtract(
            Box::new(Expression::Constant(f64::MAX)),
            Box::new(Expression::Constant(-f64::MAX)),
        ),
        Expression::Multiply(
            Box::new(Expression::Constant(f64::MAX)),
            Box::new(Expression::Constant(2.0)),
        ),
        Expression::Divide(
            Box::new(Expression::Constant(1.0)),
            Box::new(Expression::Constant(-0.0)),
        ),
        Expression::Divide(
            Box::new(Expression::Property("energy".to_owned())),
            Box::new(Expression::Subtract(
                Box::new(Expression::Constant(2.0)),
                Box::new(Expression::Constant(2.0)),
            )),
        ),
    ] {
        let automaton = automaton(vec![Rule {
            target: RuleTarget::Agent,
            condition: Condition::TypeIs(2),
            probability: Expression::Constant(0.0),
            actions: vec![set_energy(expression)],
        }]);

        let error = automaton.validate().unwrap_err();

        assert!(error.contains("Rule 0: Action 0:"), "{error}");
        assert!(
            error.contains("finite") || error.contains("Division by zero"),
            "{error}"
        );
    }
}

#[test]
fn rejects_invalid_tolerances_even_in_conditions_that_would_be_skipped() {
    for tolerance in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let condition = Condition::Any(vec![
            Condition::Always,
            Condition::Compare {
                left: Expression::Constant(0.0),
                operator: Comparison::ApproximatelyEqual { tolerance },
                right: Expression::Constant(0.0),
            },
        ]);
        let automaton = automaton(vec![change_type(RuleTarget::Cell, condition, 1.0, 1)]);

        let error = automaton.validate().unwrap_err();

        assert!(error.contains("Rule 0: Condition: Branch 1:"), "{error}");
        assert!(error.contains("tolerance"), "{error}");
    }
}

#[test]
fn checks_writes_against_the_latest_type_selected_by_actions() {
    let valid = Rule {
        target: RuleTarget::Cell,
        condition: Condition::TypeIs(0),
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::SetType { type_id: 2 },
            set_energy(Expression::Constant(5.0)),
        ],
    };
    let definition = automaton(vec![valid]);
    let mut rng = StdRng::seed_from_u64(7);
    let next = apply_rules(
        RuleTarget::Cell,
        &state(0),
        &state(0),
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();
    assert_eq!(next.state.type_id, 2);
    assert_eq!(next.state.properties["energy"], 5.0);

    for (actions, index) in [
        (
            vec![
                Action::SetType { type_id: 0 },
                set_energy(Expression::Constant(5.0)),
                Action::SetType { type_id: 2 },
            ],
            1,
        ),
        (
            vec![
                Action::SetType { type_id: 2 },
                Action::SetType { type_id: 0 },
                set_energy(Expression::Constant(5.0)),
            ],
            2,
        ),
    ] {
        let definition = automaton(vec![Rule {
            target: RuleTarget::Cell,
            condition: Condition::Always,
            probability: Expression::Constant(0.0),
            actions,
        }]);
        let error = definition.validate().unwrap_err();
        assert!(
            error.contains(&format!(
                "Action {index}: Unknown property 'energy' for type 0"
            )),
            "{error}"
        );
    }
}

#[test]
fn allows_type_changes_whose_expressions_read_the_original_states_properties() {
    let mut definition = automaton(Vec::new());
    definition.object_types.push(ObjectType {
        id: 3,
        name: "field".to_owned(),
        properties: BTreeMap::from([(
            "value".to_owned(),
            PropertyDefinition {
                default_value: 0.0,
                min_value: 0.0,
                max_value: 100.0,
                integer_only: false,
            },
        )]),
    });
    definition.rules.push(Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::SetType { type_id: 3 },
            Action::SetProperty {
                property: "value".to_owned(),
                value: Expression::Add(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(1.0)),
                ),
            },
        ],
    });
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let mut rng = StdRng::seed_from_u64(7);

    definition.validate().unwrap();
    let next = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next.state.type_id, 3);
    assert_eq!(
        next.state.properties,
        BTreeMap::from([("value".to_owned(), 13.0)])
    );
    assert_eq!(current.properties["energy"], 12.0);
}

#[test]
fn allows_intermediate_out_of_range_values_that_are_replaced_before_the_final_state() {
    let definition = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![
            set_energy(Expression::Constant(-1.0)),
            set_energy(Expression::Constant(5.0)),
        ],
    }]);
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let mut rng = StdRng::seed_from_u64(7);

    definition.validate().unwrap();
    let next = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();

    assert_eq!(next.state.properties["energy"], 5.0);
}

#[test]
fn keeps_runtime_checks_for_the_properties_of_actual_objects_and_neighbors() {
    let mut definition = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![set_energy(Expression::Constant(5.0))],
    }]);
    let mut rng = StdRng::seed_from_u64(7);
    definition.validate().unwrap();

    let error = apply_rules(
        RuleTarget::Agent,
        &state(0),
        &state(0),
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap_err();
    assert!(error.contains("Unknown property 'energy' for type 0"));

    definition.rules[0].actions = vec![set_energy(Expression::SumNeighbors {
        property: "energy".to_owned(),
    })];
    definition.validate().unwrap();
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let neighbor = state(0);
    let error = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors {
            cells: vec![&neighbor],
            ..Neighbors::default()
        },
        &definition,
        &mut rng,
    )
    .unwrap_err();
    assert!(error.contains("Neighbor 0 is missing property 'energy'"));
}

#[test]
fn keeps_runtime_checks_for_denominators_calculated_from_the_world() {
    let definition = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![set_energy(Expression::Divide(
            Box::new(Expression::Constant(1.0)),
            Box::new(Expression::Property("energy".to_owned())),
        ))],
    }]);
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 0.0)]),
    };
    let mut rng = StdRng::seed_from_u64(7);

    definition.validate().unwrap();
    let error = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap_err();

    assert!(error.contains("Rule 0: Division by zero"), "{error}");
    assert_eq!(current.properties["energy"], 0.0);
}

#[test]
fn calculates_probability_from_the_current_objects_properties() {
    let mut first = change_type(RuleTarget::Agent, Condition::TypeIs(2), 1.0, 1);
    first.probability = Expression::Divide(
        Box::new(Expression::Property("energy".to_owned())),
        Box::new(Expression::Constant(100.0)),
    );
    let definition = automaton(vec![
        first,
        change_type(RuleTarget::Agent, Condition::Always, 1.0, 0),
    ]);
    let mut rng = StdRng::seed_from_u64(42);

    for (energy, expected_type) in [(0.0, 2), (100.0, 1)] {
        let current = ObjectState {
            type_id: 2,
            properties: BTreeMap::from([("energy".to_owned(), energy)]),
        };
        let next = apply_rules(
            RuleTarget::Agent,
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut rng,
        )
        .unwrap();
        assert_eq!(next.state.type_id, expected_type);
        assert_eq!(current.properties["energy"], energy);
    }
}

#[test]
fn calculates_probability_from_the_supplied_neighbors() {
    let mut birth = change_type(RuleTarget::Cell, Condition::TypeIs(0), 1.0, 1);
    birth.probability = Expression::Divide(
        Box::new(Expression::CountNeighbors { type_id: 1 }),
        Box::new(Expression::Constant(2.0)),
    );
    let definition = automaton(vec![birth]);
    let current = state(0);
    let infected = state(1);
    let mut rng = StdRng::seed_from_u64(42);

    let unchanged = apply_rules(
        RuleTarget::Cell,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();
    let changed = apply_rules(
        RuleTarget::Cell,
        &current,
        &current,
        &Neighbors {
            cells: vec![&infected, &infected],
            ..Neighbors::default()
        },
        &definition,
        &mut rng,
    )
    .unwrap();

    assert_eq!(unchanged.state.type_id, 0);
    assert_eq!(changed.state.type_id, 1);
}

#[test]
fn validates_probability_references_and_constant_formulas_before_execution() {
    for (probability, message) in [
        (
            Expression::Property("missing".to_owned()),
            "Unknown property 'missing'",
        ),
        (
            Expression::SumNeighbors {
                property: "missing".to_owned(),
            },
            "Unknown property 'missing'",
        ),
        (
            Expression::CountNeighbors { type_id: 99 },
            "Unknown object type id 99",
        ),
        (
            Expression::Add(
                Box::new(Expression::Constant(0.6)),
                Box::new(Expression::Constant(0.6)),
            ),
            "between 0 and 1",
        ),
        (
            Expression::Divide(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(0.0)),
            ),
            "Division by zero",
        ),
        (
            Expression::Power(
                Box::new(Expression::Constant(-1.0)),
                Box::new(Expression::Constant(0.5)),
            ),
            "finite",
        ),
    ] {
        let mut rule = change_type(RuleTarget::Agent, Condition::TypeIs(2), 1.0, 1);
        rule.probability = probability;
        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains("Rule 0: probability:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn rejects_invalid_calculated_probabilities_before_drawing_randomness() {
    for (probability, energy, message) in [
        (
            Expression::Property("energy".to_owned()),
            12.0,
            "between 0 and 1",
        ),
        (
            Expression::Subtract(
                Box::new(Expression::Constant(0.0)),
                Box::new(Expression::Property("energy".to_owned())),
            ),
            12.0,
            "between 0 and 1",
        ),
        (
            Expression::Divide(
                Box::new(Expression::Constant(1.0)),
                Box::new(Expression::Property("energy".to_owned())),
            ),
            0.0,
            "Division by zero",
        ),
        (
            Expression::Power(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(1024.0)),
            ),
            100.0,
            "finite",
        ),
    ] {
        let mut rule = change_type(RuleTarget::Agent, Condition::TypeIs(2), 1.0, 1);
        rule.probability = probability;
        let definition = automaton(vec![rule]);
        definition.validate().unwrap();
        let current = ObjectState {
            type_id: 2,
            properties: BTreeMap::from([("energy".to_owned(), energy)]),
        };
        let mut rng = StdRng::seed_from_u64(42);
        let mut untouched_rng = StdRng::seed_from_u64(42);

        let error = apply_rules(
            RuleTarget::Agent,
            &current,
            &current,
            &Neighbors::default(),
            &definition,
            &mut rng,
        )
        .unwrap_err();

        assert!(error.contains("Rule 0: probability:"), "{error}");
        assert!(error.contains(message), "{error}");
        assert_eq!(rng.random::<u64>(), untouched_rng.random::<u64>());
        assert_eq!(current.properties["energy"], energy);
    }
}

#[test]
fn evaluates_dynamic_probability_only_after_the_target_and_condition_match() {
    let mut rule = change_type(RuleTarget::Agent, Condition::TypeIs(2), 1.0, 1);
    rule.probability = Expression::Divide(
        Box::new(Expression::Constant(1.0)),
        Box::new(Expression::Property("energy".to_owned())),
    );
    let definition = automaton(vec![rule]);
    let mut rng = StdRng::seed_from_u64(42);
    let current = state(0);
    let unchanged = apply_rules(
        RuleTarget::Agent,
        &current,
        &current,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();
    assert_eq!(unchanged.state.type_id, 0);

    let wrong_target = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 0.0)]),
    };
    let unchanged = apply_rules(
        RuleTarget::Cell,
        &wrong_target,
        &wrong_target,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();
    assert_eq!(unchanged.state.properties["energy"], 0.0);
}

fn spawn_action() -> Action {
    Action::SpawnAgent {
        type_id: 2,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
        properties: BTreeMap::new(),
    }
}

#[test]
fn accepts_spawn_actions_for_cells_and_agents_and_keeps_parent_type_validation() {
    for target in [RuleTarget::Cell, RuleTarget::Agent] {
        let mut rule = change_type(target, Condition::Always, 1.0, 2);
        rule.actions = vec![spawn_action()];
        let definition = automaton(vec![rule]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = apply_rules(
            target,
            &state(0),
            &state(0),
            &Neighbors::default(),
            &definition,
            &mut rng,
        )
        .unwrap();

        assert_eq!(result.state.type_id, 0);
        assert_eq!(result.spawn.unwrap().state.properties["energy"], 10.0);
    }

    let mut rule = change_type(RuleTarget::Agent, Condition::Always, 1.0, 0);
    rule.actions.extend([
        spawn_action(),
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        },
    ]);
    let error = automaton(vec![rule]).validate().unwrap_err();
    assert!(
        error.contains("Action 2: Unknown property 'energy' for type 0"),
        "{error}"
    );
}

#[test]
fn rejects_invalid_spawn_definitions_even_when_the_rule_cannot_execute() {
    let invalid_cases = [
        (Some(99), None, None, "Unknown object type id 99"),
        (
            None,
            Some(("missing", Expression::Constant(1.0))),
            None,
            "Unknown spawn property 'missing'",
        ),
        (
            None,
            Some(("energy", Expression::Property("missing".to_owned()))),
            None,
            "Unknown property 'missing'",
        ),
        (
            None,
            Some(("energy", Expression::Constant(-1.0))),
            None,
            "outside the allowed range",
        ),
        (
            None,
            None,
            Some(Expression::Constant(0.5)),
            "finite integer",
        ),
        (
            None,
            None,
            Some(Expression::Constant(f64::from(i32::MAX) + 1.0)),
            "i32 range",
        ),
    ];
    for (type_id, property, offset, message) in invalid_cases {
        let mut action = spawn_action();
        if let Action::SpawnAgent {
            type_id: target_type,
            properties,
            row_offset,
            ..
        } = &mut action
        {
            if let Some(type_id) = type_id {
                *target_type = type_id;
            }
            if let Some((name, expression)) = property {
                properties.insert(name.to_owned(), expression);
            }
            if let Some(expression) = offset {
                *row_offset = expression;
            }
        }
        let mut rule = change_type(RuleTarget::Agent, Condition::Any(Vec::new()), 0.0, 2);
        rule.actions = vec![action];

        let error = automaton(vec![rule]).validate().unwrap_err();

        assert!(error.contains("Rule 0: Action 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn rejects_multiple_spawns_and_removal_combined_with_spawning() {
    for actions in [
        vec![spawn_action(), spawn_action()],
        vec![spawn_action(), Action::RemoveAgent {}],
        vec![Action::RemoveAgent {}, spawn_action()],
    ] {
        let mut rule = change_type(RuleTarget::Agent, Condition::Always, 0.0, 2);
        let message = if actions
            .iter()
            .any(|action| matches!(action, Action::RemoveAgent {}))
        {
            "RemoveAgent must be the only action"
        } else {
            "at most one SpawnAgent action"
        };
        rule.actions = actions;

        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn validates_cell_type_and_property_references_even_in_unused_rules() {
    let base = Rule {
        target: RuleTarget::Agent,
        condition: Condition::Always,
        probability: Expression::Constant(0.0),
        actions: vec![Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        }],
    };
    let missing = Expression::CellProperty("missing".to_owned());
    let mut wrong_type = base.clone();
    wrong_type.condition =
        Condition::All(vec![Condition::Any(Vec::new()), Condition::CellTypeIs(99)]);
    let mut wrong_condition = base.clone();
    wrong_condition.condition = Condition::Compare {
        left: missing.clone(),
        operator: Comparison::Greater,
        right: Expression::Constant(0.0),
    };
    let mut wrong_probability = base.clone();
    wrong_probability.probability = missing.clone();
    let mut wrong_action = base.clone();
    wrong_action.actions = vec![Action::SetProperty {
        property: "energy".to_owned(),
        value: missing.clone(),
    }];
    let mut wrong_spawn = base;
    wrong_spawn.actions = vec![Action::SpawnAgent {
        type_id: 2,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
        properties: BTreeMap::from([("energy".to_owned(), missing)]),
    }];

    for (rule, message) in [
        (wrong_type, "Unknown object type id 99"),
        (wrong_condition, "Unknown property 'missing'"),
        (wrong_probability, "Unknown property 'missing'"),
        (wrong_action, "Unknown property 'missing'"),
        (wrong_spawn, "Unknown property 'missing'"),
    ] {
        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains("Rule 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn calculates_rule_probability_from_the_cell_instead_of_the_agents_properties() {
    let definition = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::All(vec![Condition::TypeIs(2), Condition::CellTypeIs(2)]),
        probability: Expression::Divide(
            Box::new(Expression::CellProperty("energy".to_owned())),
            Box::new(Expression::Constant(10.0)),
        ),
        actions: vec![Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(99.0),
        }],
    }]);
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 5.0)]),
    };

    for (cell_energy, expected_energy) in [(0.0, 5.0), (10.0, 99.0)] {
        let cell = ObjectState {
            type_id: 2,
            properties: BTreeMap::from([("energy".to_owned(), cell_energy)]),
        };
        let mut rng = StdRng::seed_from_u64(42);

        let result = apply_rules(
            RuleTarget::Agent,
            &current,
            &cell,
            &Neighbors::default(),
            &definition,
            &mut rng,
        )
        .unwrap();

        assert_eq!(result.state.properties["energy"], expected_energy);
        assert_eq!(current.properties["energy"], 5.0);
    }
}

#[test]
fn reports_actual_cell_property_errors_and_allows_a_type_guard_to_skip_them() {
    let mut definition = automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::CellProperty("energy".to_owned()),
        }],
    }]);
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let cell = state(0);
    let mut rng = StdRng::seed_from_u64(42);

    let error = apply_rules(
        RuleTarget::Agent,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap_err();
    assert!(
        error.contains("Rule 0: Current cell is missing property 'energy'"),
        "{error}"
    );
    definition.rules[0].condition =
        Condition::All(vec![Condition::TypeIs(2), Condition::CellTypeIs(2)]);
    let result = apply_rules(
        RuleTarget::Agent,
        &current,
        &cell,
        &Neighbors::default(),
        &definition,
        &mut rng,
    )
    .unwrap();
    assert_eq!(result.state.properties["energy"], 12.0);
    assert!(cell.properties.is_empty());
}

#[test]
fn cell_writing_actions_are_agent_only_even_in_unused_rules() {
    for action in [
        Action::SetCellProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        },
        Action::SetCellType { type_id: 2 },
    ] {
        let rule = Rule {
            target: RuleTarget::Cell,
            condition: Condition::Any(Vec::new()),
            probability: Expression::Constant(0.0),
            actions: vec![action],
        };

        let error = automaton(vec![rule]).validate().unwrap_err();

        assert!(error.contains("Rule 0: Action 0:"), "{error}");
        assert!(error.contains("only allowed in agent rules"), "{error}");
    }
}

#[test]
fn validates_object_and_cell_type_changes_independently() {
    let set_energy = Action::SetProperty {
        property: "energy".to_owned(),
        value: Expression::Constant(1.0),
    };
    let set_cell_energy = Action::SetCellProperty {
        property: "energy".to_owned(),
        value: Expression::Constant(2.0),
    };
    let base = Rule {
        target: RuleTarget::Agent,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::SetType { type_id: 2 },
            Action::SetCellType { type_id: 2 },
            set_energy.clone(),
            set_cell_energy.clone(),
        ],
    };
    automaton(vec![base.clone()]).validate().unwrap();

    for (actions, message) in [
        (
            vec![
                Action::SetType { type_id: 2 },
                Action::SetCellType { type_id: 1 },
                set_energy.clone(),
                set_cell_energy.clone(),
            ],
            "Unknown cell property 'energy' for type 1",
        ),
        (
            vec![
                Action::SetType { type_id: 1 },
                Action::SetCellType { type_id: 2 },
                set_cell_energy.clone(),
                set_energy,
            ],
            "Unknown property 'energy' for type 1",
        ),
        (
            vec![
                Action::SetCellType { type_id: 2 },
                Action::SetCellType { type_id: 0 },
                set_cell_energy,
            ],
            "Unknown cell property 'energy' for type 0",
        ),
    ] {
        let mut rule = base.clone();
        rule.actions = actions;
        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn validates_cell_write_references_and_rejects_removal_combinations() {
    for (actions, message) in [
        (
            vec![Action::SetCellType { type_id: 99 }],
            "Unknown object type id 99",
        ),
        (
            vec![Action::SetCellProperty {
                property: "missing".to_owned(),
                value: Expression::Constant(0.0),
            }],
            "Unknown property 'missing'",
        ),
        (
            vec![Action::SetCellProperty {
                property: "energy".to_owned(),
                value: Expression::CellProperty("missing".to_owned()),
            }],
            "Unknown property 'missing'",
        ),
        (
            vec![Action::RemoveAgent {}, Action::SetCellType { type_id: 0 }],
            "RemoveAgent must be the only action",
        ),
        (
            vec![
                Action::SetCellProperty {
                    property: "energy".to_owned(),
                    value: Expression::Constant(0.0),
                },
                Action::RemoveAgent {},
            ],
            "RemoveAgent must be the only action",
        ),
    ] {
        let rule = Rule {
            target: RuleTarget::Agent,
            condition: Condition::Always,
            probability: Expression::Constant(0.0),
            actions,
        };
        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains("Rule 0: Action"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn unmatched_or_failed_probability_rules_do_not_request_cell_updates() {
    let base = Rule {
        target: RuleTarget::Agent,
        condition: Condition::Always,
        probability: Expression::Constant(0.0),
        actions: vec![Action::SetCellProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        }],
    };
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let cell = state(0);
    let mut false_condition = base.clone();
    false_condition.condition = Condition::Any(Vec::new());
    false_condition.probability = Expression::Constant(1.0);

    for rule in [base, false_condition] {
        let definition = automaton(vec![rule]);
        let result = apply_rules(
            RuleTarget::Agent,
            &current,
            &cell,
            &Neighbors::default(),
            &definition,
            &mut StdRng::seed_from_u64(42),
        )
        .unwrap();
        assert!(result.cell_update.is_none());
        assert_eq!(result.state.properties["energy"], 12.0);
    }
}

#[test]
fn validates_neighbor_agent_types_and_properties_on_the_selected_type() {
    for (expression, message) in [
        (
            Expression::CountNeighborAgents { type_id: 99 },
            "Unknown object type id 99",
        ),
        (
            Expression::SumNeighborAgents {
                type_id: 99,
                property: "energy".to_owned(),
            },
            "Unknown object type id 99",
        ),
        (
            Expression::SumNeighborAgents {
                type_id: 1,
                property: "energy".to_owned(),
            },
            "Unknown property 'energy' for neighbor agent type 1",
        ),
        (
            Expression::SumNeighborAgents {
                type_id: 2,
                property: "missing".to_owned(),
            },
            "Unknown property 'missing' for neighbor agent type 2",
        ),
    ] {
        let rule = Rule {
            target: RuleTarget::Agent,
            condition: Condition::Any(Vec::new()),
            probability: Expression::Constant(0.0),
            actions: vec![Action::SetProperty {
                property: "energy".to_owned(),
                value: expression,
            }],
        };
        let error = automaton(vec![rule]).validate().unwrap_err();
        assert!(error.contains("Rule 0: Action 0:"), "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn computes_probabilities_from_neighbor_agent_counts_and_sums() {
    let current = ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), 12.0)]),
    };
    let first = Agent {
        id: 7,
        position: Position { row: 0, col: 1 },
        state: ObjectState {
            type_id: 2,
            properties: BTreeMap::from([("energy".to_owned(), 10.0)]),
        },
    };
    let mut second = first.clone();
    second.id = 8;
    let neighbors = Neighbors {
        agents: vec![&first, &second],
        ..Neighbors::default()
    };

    for probability in [
        Expression::Divide(
            Box::new(Expression::CountNeighborAgents { type_id: 2 }),
            Box::new(Expression::Constant(2.0)),
        ),
        Expression::Divide(
            Box::new(Expression::SumNeighborAgents {
                type_id: 2,
                property: "energy".to_owned(),
            }),
            Box::new(Expression::Constant(20.0)),
        ),
    ] {
        let definition = automaton(vec![Rule {
            target: RuleTarget::Agent,
            condition: Condition::Always,
            probability,
            actions: vec![Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::Constant(99.0),
            }],
        }]);
        for (observed, expected) in [(&Neighbors::default(), 12.0), (&neighbors, 99.0)] {
            let result = apply_rules(
                RuleTarget::Agent,
                &current,
                &state(0),
                observed,
                &definition,
                &mut StdRng::seed_from_u64(42),
            )
            .unwrap();
            assert_eq!(result.state.properties["energy"], expected);
        }
    }
}
