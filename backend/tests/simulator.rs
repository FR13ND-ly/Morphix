mod common;

use std::collections::BTreeMap;

use backend::{
    models::{
        action::Action,
        agent::Agent,
        automaton::{AutomatonDefinition, Boundary},
        condition::{Comparison, Condition},
        expression::Expression,
        neighborhood::Neighborhood,
        object_type::{ObjectType, PropertyDefinition},
        position::Position,
        rule::{Rule, RuleTarget},
        simulation::SimulationRequest,
        state::ObjectState,
        world::World,
    },
    simulator::{run, step},
};
use rand::{SeedableRng, rngs::StdRng};

fn state(type_id: u8) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::new(),
    }
}

fn compare(left: Expression, operator: Comparison, right: f64) -> Condition {
    Condition::Compare {
        left,
        operator,
        right: Expression::Constant(right),
    }
}

fn set_property(target: RuleTarget, property: &str, value: Expression) -> Rule {
    Rule {
        target,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetProperty {
            property: property.to_owned(),
            value,
        }],
    }
}

fn game_of_life() -> AutomatonDefinition {
    let live_count = Expression::CountNeighbors { type_id: 1 };

    AutomatonDefinition {
        name: "Game of Life".to_owned(),
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
        rules: vec![
            Rule {
                target: RuleTarget::Cell,
                condition: Condition::All(vec![
                    Condition::TypeIs(0),
                    compare(live_count.clone(), Comparison::Equal, 3.0),
                ]),
                probability: Expression::Constant(1.0),
                actions: vec![Action::SetType { type_id: 1 }],
            },
            Rule {
                target: RuleTarget::Cell,
                condition: Condition::All(vec![
                    Condition::TypeIs(1),
                    Condition::Any(vec![
                        compare(live_count.clone(), Comparison::Less, 2.0),
                        compare(live_count, Comparison::Greater, 3.0),
                    ]),
                ]),
                probability: Expression::Constant(1.0),
                actions: vec![Action::SetType { type_id: 0 }],
            },
        ],
    }
}

fn life_world(height: usize, width: usize, live_positions: &[(usize, usize)]) -> World {
    let mut world = World {
        cells: vec![vec![state(0); width]; height],
        agents: Vec::new(),
    };

    for &(row, col) in live_positions {
        world.cells[row][col] = state(1);
    }

    world
}

fn type_grid(world: &World) -> Vec<Vec<u8>> {
    world
        .cells
        .iter()
        .map(|row| row.iter().map(|cell| cell.type_id).collect())
        .collect()
}

#[test]
fn rule_30_configuration_matches_all_eight_left_center_right_combinations() {
    let request: SimulationRequest = serde_json::from_value(common::directional_cells()).unwrap();
    for reversed in [false, true] {
        let mut automaton = request.automaton.clone();
        if reversed {
            automaton.neighborhood.offsets.reverse();
        }
        for (pattern, expected) in [
            ([1, 1, 1], 0),
            ([1, 1, 0], 0),
            ([1, 0, 1], 0),
            ([1, 0, 0], 1),
            ([0, 1, 1], 1),
            ([0, 1, 0], 1),
            ([0, 0, 1], 1),
            ([0, 0, 0], 0),
        ] {
            let initial = World {
                cells: vec![pattern.into_iter().map(state).collect()],
                agents: Vec::new(),
            };
            let mut rng = StdRng::seed_from_u64(42);
            let next = step(&automaton, &initial, &mut rng).unwrap();
            assert_eq!(
                next.cells[0][1].type_id, expected,
                "pattern {pattern:?}, reversed {reversed}"
            );
        }
    }
}

#[test]
fn rule_30_example_produces_the_reference_generations_from_one_live_cell() {
    let request: SimulationRequest = serde_json::from_value(common::directional_cells()).unwrap();
    let history = run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        true,
    )
    .unwrap();
    let expected = [
        "00000100000",
        "00001110000",
        "00011001000",
        "00110111100",
        "01100100010",
    ];
    assert_eq!(history.len(), expected.len());
    for (index, (snapshot, expected)) in history.iter().zip(expected).enumerate() {
        let expected: Vec<u8> = expected.bytes().map(|value| value - b'0').collect();
        assert_eq!(snapshot.generation, index);
        assert_eq!(type_grid(&snapshot.world), vec![expected]);
    }
}

#[test]
fn directional_rule_30_reads_apply_fixed_and_wrapped_boundaries_on_one_cell_worlds() {
    let request: SimulationRequest = serde_json::from_value(common::directional_cells()).unwrap();
    let initial = life_world(1, 1, &[(0, 0)]);
    for (boundary, expected) in [
        (Boundary::Fixed { state: state(0) }, 1),
        (Boundary::Wrap, 0),
    ] {
        let mut automaton = request.automaton.clone();
        automaton.boundary = boundary;
        let mut rng = StdRng::seed_from_u64(42);
        let next = step(&automaton, &initial, &mut rng).unwrap();
        assert_eq!(next.cells[0][0].type_id, expected);
    }
}

fn numeric_type(id: u8, name: &str, property: &str) -> ObjectType {
    ObjectType {
        id,
        name: name.to_owned(),
        properties: BTreeMap::from([(
            property.to_owned(),
            PropertyDefinition {
                default_value: 0.0,
                min_value: 0.0,
                max_value: 100.0,
                integer_only: false,
            },
        )]),
    }
}

fn numeric_state(type_id: u8, property: &str, value: f64) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::from([(property.to_owned(), value)]),
    }
}

fn numeric_automaton(rules: Vec<Rule>) -> AutomatonDefinition {
    AutomatonDefinition {
        name: "Numeric example".to_owned(),
        object_types: vec![
            numeric_type(0, "field", "value"),
            numeric_type(1, "rabbit", "energy"),
        ],
        neighborhood: Neighborhood {
            offsets: vec![(0, -1), (0, 1)],
        },
        boundary: Boundary::Wrap,
        rules,
    }
}

fn numeric_world() -> World {
    World {
        cells: vec![vec![
            numeric_state(0, "value", 1.0),
            numeric_state(0, "value", 2.0),
            numeric_state(0, "value", 3.0),
        ]],
        agents: vec![
            Agent {
                id: 7,
                position: Position { row: 0, col: 1 },
                state: numeric_state(1, "energy", 12.0),
            },
            Agent {
                id: 9,
                position: Position { row: 0, col: 0 },
                state: numeric_state(1, "energy", 12.0),
            },
        ],
    }
}

#[test]
fn agents_read_directional_properties_from_the_old_world_and_their_start_position() {
    let left_value = Expression::NeighborProperty {
        offset: (0, -1),
        property: "value".to_owned(),
    };
    let automaton = numeric_automaton(vec![
        set_property(
            RuleTarget::Cell,
            "value",
            Expression::Add(
                Box::new(Expression::Property("value".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::All(vec![
                Condition::TypeIs(1),
                Condition::NeighborTypeIs {
                    offset: (0, -1),
                    type_id: 0,
                },
                compare(left_value.clone(), Comparison::Greater, 0.0),
            ]),
            probability: Expression::Divide(
                Box::new(left_value.clone()),
                Box::new(left_value.clone()),
            ),
            actions: vec![
                Action::SetProperty {
                    property: "energy".to_owned(),
                    value: left_value,
                },
                Action::Move {
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(1.0),
                },
            ],
        },
    ]);
    let initial = numeric_world();
    let history = run(&automaton, &initial, 2, 42, true).unwrap();

    assert_eq!(history[1].world.agents[0].state.properties["energy"], 1.0);
    assert_eq!(history[1].world.agents[1].state.properties["energy"], 3.0);
    assert_eq!(history[1].world.agents[0].position.col, 2);
    assert_eq!(history[1].world.agents[1].position.col, 1);
    assert_eq!(history[2].world.agents[0].state.properties["energy"], 3.0);
    assert_eq!(history[2].world.agents[1].state.properties["energy"], 2.0);
    assert_eq!(history[2].world.agents[0].position.col, 0);
    assert_eq!(history[2].world.agents[1].position.col, 2);
    assert_eq!(initial.cells[0][0].properties["value"], 1.0);
}

#[test]
fn reads_the_old_cell_at_the_agents_start_position_and_recalculates_after_movement() {
    let definition = numeric_automaton(vec![
        set_property(
            RuleTarget::Cell,
            "value",
            Expression::Add(
                Box::new(Expression::CellProperty("value".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::All(vec![Condition::TypeIs(1), Condition::CellTypeIs(0)]),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetProperty {
                    property: "energy".to_owned(),
                    value: Expression::Add(
                        Box::new(Expression::Property("energy".to_owned())),
                        Box::new(Expression::CellProperty("value".to_owned())),
                    ),
                },
                Action::Move {
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(1.0),
                },
            ],
        },
    ]);
    let initial = numeric_world();

    let history = run(&definition, &initial, 2, 42, true).unwrap();

    assert_eq!(history[1].world.cells[0][1].properties["value"], 3.0);
    assert_eq!(history[1].world.agents[0].state.properties["energy"], 14.0);
    assert_eq!(history[1].world.agents[1].state.properties["energy"], 13.0);
    assert_eq!(history[1].world.agents[0].position.col, 2);
    assert_eq!(history[1].world.agents[1].position.col, 1);
    assert_eq!(history[2].world.agents[0].state.properties["energy"], 18.0);
    assert_eq!(history[2].world.agents[1].state.properties["energy"], 16.0);
    assert_eq!(initial.cells[0][1].properties["value"], 2.0);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
    assert_eq!(initial.agents[0].position.col, 1);
}

#[test]
fn reads_old_cell_types_and_skips_missing_properties_on_other_terrain() {
    let mut definition = numeric_automaton(vec![
        Rule {
            target: RuleTarget::Cell,
            condition: Condition::TypeIs(0),
            probability: Expression::Constant(1.0),
            actions: vec![Action::SetType { type_id: 2 }],
        },
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::All(vec![
                Condition::CellTypeIs(0),
                compare(
                    Expression::CellProperty("value".to_owned()),
                    Comparison::Greater,
                    0.0,
                ),
            ]),
            probability: Expression::Constant(1.0),
            actions: vec![Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::Add(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::CellProperty("value".to_owned())),
                ),
            }],
        },
    ]);
    definition.object_types.push(ObjectType {
        id: 2,
        name: "rock".to_owned(),
        properties: BTreeMap::new(),
    });
    let mut initial = numeric_world();
    initial.cells[0][1] = state(2);

    let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();

    assert!(
        next.cells[0]
            .iter()
            .all(|cell| cell.type_id == 2 && cell.properties.is_empty())
    );
    assert_eq!(next.agents[0].state.properties["energy"], 12.0);
    assert_eq!(next.agents[1].state.properties["energy"], 13.0);
    assert_eq!(initial.cells[0][0].type_id, 0);
    definition.validate_world(&next).unwrap();
}

#[test]
fn terrain_controls_rule_probability_and_the_initial_state_of_a_new_agent() {
    let definition = numeric_automaton(vec![Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(1),
        probability: Expression::CellProperty("value".to_owned()),
        actions: vec![
            Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::CellProperty("value".to_owned()),
            },
            Action::SpawnAgent {
                type_id: 1,
                row_offset: Expression::Constant(0.0),
                col_offset: Expression::Constant(1.0),
                properties: BTreeMap::from([(
                    "energy".to_owned(),
                    Expression::CellProperty("value".to_owned()),
                )]),
            },
        ],
    }]);
    let mut initial = numeric_world();
    for (col, cell) in initial.cells[0].iter_mut().enumerate() {
        cell.properties.insert("value".to_owned(), col as f64);
    }

    let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();

    assert_eq!(next.agents.len(), 3);
    assert_eq!(next.agents[0].state.properties["energy"], 1.0);
    assert_eq!(next.agents[1].state.properties["energy"], 12.0);
    assert_eq!(next.agents[2].id, 10);
    assert_eq!(next.agents[2].position.col, 2);
    assert_eq!(next.agents[2].state.properties["energy"], 1.0);
    assert_eq!(next.cells[0][1].properties["value"], 1.0);
    assert_eq!(initial.agents.len(), 2);
}

fn feeding_rule() -> Rule {
    Rule {
        target: RuleTarget::Agent,
        condition: Condition::All(vec![
            Condition::TypeIs(1),
            Condition::CellTypeIs(0),
            compare(
                Expression::CellProperty("value".to_owned()),
                Comparison::Greater,
                0.0,
            ),
        ]),
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::Add(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::CellProperty("value".to_owned())),
                ),
            },
            Action::SetCellProperty {
                property: "value".to_owned(),
                value: Expression::Constant(0.0),
            },
        ],
    }
}

#[test]
fn consumes_food_at_the_start_position_and_preserves_other_autonomous_cell_changes() {
    let mut feeding = feeding_rule();
    feeding.actions.push(Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
    });
    let mut growth = set_property(
        RuleTarget::Cell,
        "value",
        Expression::Add(
            Box::new(Expression::Property("value".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    );
    growth.actions.push(Action::SetProperty {
        property: "temperature".to_owned(),
        value: Expression::Add(
            Box::new(Expression::Property("temperature".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    });
    let mut definition = numeric_automaton(vec![growth, feeding]);
    definition.object_types[0].properties.insert(
        "temperature".to_owned(),
        PropertyDefinition {
            default_value: 0.0,
            min_value: 0.0,
            max_value: 100.0,
            integer_only: false,
        },
    );
    let mut initial = numeric_world();
    for cell in &mut initial.cells[0] {
        cell.properties.insert("temperature".to_owned(), 5.0);
    }

    let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();

    assert_eq!(next.agents[0].state.properties["energy"], 14.0);
    assert_eq!(next.agents[1].state.properties["energy"], 13.0);
    assert_eq!(next.agents[0].position.col, 2);
    assert_eq!(next.agents[1].position.col, 1);
    assert_eq!(next.cells[0][0].properties["value"], 0.0);
    assert_eq!(next.cells[0][1].properties["value"], 0.0);
    assert_eq!(next.cells[0][2].properties["value"], 4.0);
    assert!(
        next.cells[0]
            .iter()
            .all(|cell| cell.properties["temperature"] == 6.0)
    );
    assert_eq!(initial.cells[0][1].properties["value"], 2.0);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
}

#[test]
fn conflicting_agent_cell_updates_abort_the_run_in_either_agent_order() {
    let definition = numeric_automaton(vec![feeding_rule()]);
    let mut initial = numeric_world();
    initial.agents[1].position = initial.agents[0].position;

    for _ in 0..2 {
        let original = serde_json::to_value(&initial).unwrap();
        let error = run(&definition, &initial, 1, 42, true).unwrap_err();
        assert!(
            error.contains("Generation 1: Cell (0, 1): Conflicting cell updates"),
            "{error}"
        );
        assert!(
            error.contains("agents 7 and 9") || error.contains("agents 9 and 7"),
            "{error}"
        );
        assert_eq!(serde_json::to_value(&initial).unwrap(), original);
        initial.agents.reverse();
    }
}

#[test]
fn inactive_agents_do_not_claim_cells_and_blocked_moves_keep_food_and_energy_changes() {
    let mut feeding = feeding_rule();
    if let Condition::All(conditions) = &mut feeding.condition {
        conditions.push(compare(
            Expression::Property("energy".to_owned()),
            Comparison::Greater,
            0.0,
        ));
    }
    feeding.actions.push(Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
    });
    let definition = numeric_automaton(vec![feeding]);
    for other_col in [0, 1] {
        let mut initial = numeric_world();
        initial.agents[0].position.col = 0;
        initial.agents[1].position.col = other_col;
        initial.agents[1]
            .state
            .properties
            .insert("energy".to_owned(), 0.0);

        let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();

        assert_eq!(next.agents[0].state.properties["energy"], 13.0);
        assert_eq!(
            next.agents[0].position.col,
            if other_col == 0 { 1 } else { 0 }
        );
        assert_eq!(next.agents[1].state.properties["energy"], 0.0);
        assert_eq!(next.cells[0][0].properties["value"], 0.0);
    }
}

#[test]
fn changes_cell_type_at_the_old_position_while_the_agent_moves() {
    let mut definition = game_of_life();
    definition.object_types.push(ObjectType {
        id: 2,
        name: "ant".to_owned(),
        properties: BTreeMap::new(),
    });
    definition.rules = vec![
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::All(vec![Condition::TypeIs(2), Condition::CellTypeIs(0)]),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetCellType { type_id: 1 },
                Action::Move {
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(1.0),
                },
            ],
        },
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::All(vec![Condition::TypeIs(2), Condition::CellTypeIs(1)]),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetCellType { type_id: 0 },
                Action::Move {
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(-1.0),
                },
            ],
        },
    ];
    let mut initial = life_world(1, 3, &[(0, 1)]);
    initial.agents.push(Agent {
        id: 7,
        position: Position { row: 0, col: 0 },
        state: state(2),
    });

    let history = run(&definition, &initial, 2, 42, true).unwrap();

    assert_eq!(type_grid(&history[0].world), vec![vec![0, 1, 0]]);
    assert_eq!(type_grid(&history[1].world), vec![vec![1, 1, 0]]);
    assert_eq!(type_grid(&history[2].world), vec![vec![1, 0, 0]]);
    assert_eq!(history[1].world.agents[0].position.col, 1);
    assert_eq!(history[2].world.agents[0].position.col, 0);
    assert_eq!(history[2].world.agents[0].state.type_id, 2);
}

#[test]
fn rejects_cell_property_updates_incompatible_with_the_autonomously_changed_type() {
    let mut definition = numeric_automaton(vec![
        Rule {
            target: RuleTarget::Cell,
            condition: Condition::TypeIs(0),
            probability: Expression::Constant(1.0),
            actions: vec![Action::SetType { type_id: 2 }],
        },
        feeding_rule(),
    ]);
    definition.object_types.push(ObjectType {
        id: 2,
        name: "rock".to_owned(),
        properties: BTreeMap::new(),
    });
    let initial = numeric_world();

    let error = run(&definition, &initial, 1, 42, true).unwrap_err();

    assert!(error.contains("Generation 1: Cell (0, 1): Agent 7: Cell update: Unknown cell property 'value' for type 2"), "{error}");
    assert_eq!(initial.cells[0][1].type_id, 0);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
}

#[test]
fn an_explicit_agent_cell_type_change_overrides_the_autonomous_type_change() {
    let mut definition = numeric_automaton(vec![
        Rule {
            target: RuleTarget::Cell,
            condition: Condition::TypeIs(0),
            probability: Expression::Constant(1.0),
            actions: vec![Action::SetType { type_id: 2 }],
        },
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::TypeIs(1),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetCellType { type_id: 0 },
                Action::SetCellProperty {
                    property: "value".to_owned(),
                    value: Expression::Constant(5.0),
                },
            ],
        },
    ]);
    definition.object_types.push(ObjectType {
        id: 2,
        name: "rock".to_owned(),
        properties: BTreeMap::new(),
    });

    let next = step(
        &definition,
        &numeric_world(),
        &mut StdRng::seed_from_u64(42),
    )
    .unwrap();

    assert_eq!(next.cells[0][0].type_id, 0);
    assert_eq!(next.cells[0][1].properties["value"], 5.0);
    assert_eq!(next.cells[0][2].type_id, 2);
    definition.validate_world(&next).unwrap();
}

#[test]
fn creating_a_child_still_reads_old_food_after_requesting_its_consumption() {
    let mut feeding = feeding_rule();
    feeding.actions.push(Action::SpawnAgent {
        type_id: 1,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(1.0),
        properties: BTreeMap::from([(
            "energy".to_owned(),
            Expression::CellProperty("value".to_owned()),
        )]),
    });
    let definition = numeric_automaton(vec![feeding]);
    let mut initial = numeric_world();
    initial.agents.truncate(1);

    let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();

    assert_eq!(next.cells[0][1].properties["value"], 0.0);
    assert_eq!(next.agents[0].state.properties["energy"], 14.0);
    assert_eq!(next.agents[1].id, 8);
    assert_eq!(next.agents[1].position.col, 2);
    assert_eq!(next.agents[1].state.properties["energy"], 2.0);
    assert_eq!(initial.agents.len(), 1);
}

#[test]
fn cells_and_agents_observe_old_agent_states_and_positions_for_the_entire_step() {
    let definition = numeric_automaton(vec![
        set_property(
            RuleTarget::Cell,
            "value",
            Expression::CountNeighborAgents { type_id: 1 },
        ),
        Rule {
            target: RuleTarget::Agent,
            condition: compare(
                Expression::CountNeighborAgents { type_id: 1 },
                Comparison::Equal,
                1.0,
            ),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetProperty {
                    property: "energy".to_owned(),
                    value: Expression::Add(
                        Box::new(Expression::Property("energy".to_owned())),
                        Box::new(Expression::SumNeighborAgents {
                            type_id: 1,
                            property: "energy".to_owned(),
                        }),
                    ),
                },
                Action::Move {
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(1.0),
                },
            ],
        },
    ]);
    let initial = numeric_world();

    let history = run(&definition, &initial, 2, 42, true).unwrap();

    assert_eq!(
        history[1].world.cells[0]
            .iter()
            .map(|cell| cell.properties["value"])
            .collect::<Vec<_>>(),
        vec![1.0, 1.0, 2.0]
    );
    assert_eq!(
        history[2].world.cells[0]
            .iter()
            .map(|cell| cell.properties["value"])
            .collect::<Vec<_>>(),
        vec![2.0, 1.0, 1.0]
    );
    assert!(
        history[1]
            .world
            .agents
            .iter()
            .all(|agent| agent.state.properties["energy"] == 24.0)
    );
    assert!(
        history[2]
            .world
            .agents
            .iter()
            .all(|agent| agent.state.properties["energy"] == 48.0)
    );
    assert_eq!(history[1].world.agents[0].position.col, 2);
    assert_eq!(history[1].world.agents[1].position.col, 1);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
    assert_eq!(initial.agents[0].position.col, 1);
}

#[test]
fn observations_include_agents_removed_this_step_and_exclude_newborn_agents() {
    let definition = numeric_automaton(vec![
        Rule {
            target: RuleTarget::Agent,
            condition: compare(
                Expression::Property("energy".to_owned()),
                Comparison::Equal,
                0.0,
            ),
            probability: Expression::Constant(1.0),
            actions: vec![Action::RemoveAgent {}],
        },
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::TypeIs(1),
            probability: Expression::Constant(1.0),
            actions: vec![
                Action::SetProperty {
                    property: "energy".to_owned(),
                    value: Expression::CountNeighborAgents { type_id: 1 },
                },
                Action::SpawnAgent {
                    type_id: 1,
                    row_offset: Expression::Constant(0.0),
                    col_offset: Expression::Constant(1.0),
                    properties: BTreeMap::from([("energy".to_owned(), Expression::Constant(10.0))]),
                },
            ],
        },
    ]);
    let mut initial = numeric_world();
    initial.agents[1]
        .state
        .properties
        .insert("energy".to_owned(), 0.0);

    for _ in 0..2 {
        let next = step(&definition, &initial, &mut StdRng::seed_from_u64(42)).unwrap();
        assert_eq!(next.agents.len(), 2);
        let parent = next.agents.iter().find(|agent| agent.id == 7).unwrap();
        assert_eq!(parent.state.properties["energy"], 1.0);
        let child = next.agents.iter().find(|agent| agent.id == 10).unwrap();
        assert_eq!(child.position.col, 2);
        assert_eq!(child.state.properties["energy"], 10.0);
        initial.agents.reverse();
    }
}

#[test]
fn advances_a_game_of_life_blinker_synchronously_and_preserves_the_input() {
    let automaton = game_of_life();
    let current = life_world(5, 5, &[(2, 1), (2, 2), (2, 3)]);
    let original = type_grid(&current);
    let expected = life_world(5, 5, &[(1, 2), (2, 2), (3, 2)]);
    let mut rng = StdRng::seed_from_u64(7);

    let first = step(&automaton, &current, &mut rng).unwrap();
    let second = step(&automaton, &first, &mut rng).unwrap();

    assert_eq!(type_grid(&first), type_grid(&expected));
    assert_eq!(type_grid(&second), original);
    assert_eq!(type_grid(&current), original);
    automaton.validate_world(&first).unwrap();
    automaton.validate_world(&second).unwrap();
}

#[test]
fn preserves_a_stable_game_of_life_block() {
    let automaton = game_of_life();
    let current = life_world(5, 5, &[(1, 1), (1, 2), (2, 1), (2, 2)]);
    let mut rng = StdRng::seed_from_u64(7);

    let next = step(&automaton, &current, &mut rng).unwrap();

    assert_eq!(type_grid(&next), type_grid(&current));
}

#[test]
fn uses_the_configured_boundary_when_advancing_edge_cells() {
    let mut automaton = game_of_life();
    let current = life_world(5, 5, &[(0, 1), (0, 2), (0, 3)]);
    let mut rng = StdRng::seed_from_u64(7);

    let fixed = step(&automaton, &current, &mut rng).unwrap();
    assert_eq!(
        type_grid(&fixed),
        type_grid(&life_world(5, 5, &[(0, 2), (1, 2)]))
    );

    automaton.boundary = Boundary::Wrap;
    let wrapped = step(&automaton, &current, &mut rng).unwrap();
    assert_eq!(
        type_grid(&wrapped),
        type_grid(&life_world(5, 5, &[(0, 2), (1, 2), (4, 2)]))
    );
}

#[test]
fn updates_agents_using_the_old_cells_and_preserves_their_identity_and_position() {
    let automaton = numeric_automaton(vec![
        set_property(RuleTarget::Cell, "value", Expression::Constant(0.0)),
        set_property(
            RuleTarget::Agent,
            "energy",
            Expression::SumNeighbors {
                property: "value".to_owned(),
            },
        ),
    ]);
    let current = numeric_world();
    let mut rng = StdRng::seed_from_u64(7);

    let next = step(&automaton, &current, &mut rng).unwrap();

    assert!(
        next.cells[0]
            .iter()
            .all(|cell| cell.properties["value"] == 0.0)
    );
    assert_eq!(next.agents.len(), 2);
    assert_eq!(next.agents[0].state.properties["energy"], 4.0);
    assert_eq!(next.agents[1].state.properties["energy"], 5.0);
    for (old, new) in current.agents.iter().zip(&next.agents) {
        assert_eq!(new.id, old.id);
        assert_eq!(new.position, old.position);
        assert_eq!(new.state.type_id, old.state.type_id);
        assert_eq!(old.state.properties["energy"], 12.0);
    }
    assert_eq!(current.cells[0][0].properties["value"], 1.0);
    assert_eq!(current.cells[0][1].properties["value"], 2.0);
    assert_eq!(current.cells[0][2].properties["value"], 3.0);
    automaton.validate_world(&next).unwrap();
}

#[test]
fn diffusion_example_matches_synchronous_generations_including_fractional_values() {
    let request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/diffusion.json")).unwrap();
    request.validate_limits().unwrap();
    let history = run(
        &request.automaton,
        &request.initial_world,
        request.generations,
        request.seed,
        request.save_history,
    )
    .unwrap();
    let expected = [
        [[0.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 0.0]],
        [[0.0, 20.0, 0.0], [20.0, 20.0, 20.0], [0.0, 20.0, 0.0]],
        [[8.0, 12.0, 8.0], [12.0, 20.0, 12.0], [8.0, 12.0, 8.0]],
        [[9.6, 12.0, 9.6], [12.0, 13.6, 12.0], [9.6, 12.0, 9.6]],
    ];

    assert_eq!(history.len(), expected.len());
    for (generation, (snapshot, expected_grid)) in history.iter().zip(expected).enumerate() {
        assert_eq!(snapshot.generation, generation);
        assert!(snapshot.world.agents.is_empty());
        for (row, expected_row) in snapshot.world.cells.iter().zip(expected_grid) {
            for (cell, expected_value) in row.iter().zip(expected_row) {
                assert_eq!(cell.type_id, 0);
                assert!((cell.properties["temperature"] - expected_value).abs() < 1e-10);
            }
        }
    }
}

#[test]
fn wrapped_diffusion_conserves_heat_and_converges_to_the_initial_average() {
    let request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/diffusion.json")).unwrap();
    let history = run(
        &request.automaton,
        &request.initial_world,
        100,
        request.seed,
        true,
    )
    .unwrap();
    let mut lower_bound = 0.0;
    let mut upper_bound = 100.0;

    for snapshot in &history {
        let values: Vec<_> = snapshot
            .world
            .cells
            .iter()
            .flatten()
            .map(|cell| cell.properties["temperature"])
            .collect();
        assert!((values.iter().sum::<f64>() - 100.0).abs() < 1e-9);
        assert!(values.iter().all(|value| {
            value.is_finite() && *value >= lower_bound - 1e-9 && *value <= upper_bound + 1e-9
        }));
        lower_bound = values.iter().copied().fold(f64::INFINITY, f64::min);
        upper_bound = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    }

    for cell in history.last().unwrap().world.cells.iter().flatten() {
        assert!((cell.properties["temperature"] - 100.0 / 9.0).abs() < 1e-9);
    }
}

#[test]
fn fixed_zero_boundary_allows_heat_to_leave_the_diffusion_grid() {
    let mut request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/diffusion.json")).unwrap();
    request.automaton.boundary = Boundary::Fixed {
        state: numeric_state(0, "temperature", 0.0),
    };
    let history = run(
        &request.automaton,
        &request.initial_world,
        2,
        request.seed,
        true,
    )
    .unwrap();
    let cooled = &history[2].world;
    let expected = [[8.0, 8.0, 8.0], [8.0, 20.0, 8.0], [8.0, 8.0, 8.0]];
    for (row, expected_row) in cooled.cells.iter().zip(expected) {
        for (cell, expected_value) in row.iter().zip(expected_row) {
            assert!((cell.properties["temperature"] - expected_value).abs() < 1e-10);
        }
    }
    let total: f64 = cooled
        .cells
        .iter()
        .flatten()
        .map(|cell| cell.properties["temperature"])
        .sum();
    assert!((total - 84.0).abs() < 1e-10);
}

#[test]
fn reproduces_a_sequence_of_probabilistic_generations_with_the_same_seed() {
    let mut automaton = game_of_life();
    automaton.rules = vec![Rule {
        target: RuleTarget::Cell,
        condition: Condition::TypeIs(0),
        probability: Expression::Constant(0.35),
        actions: vec![Action::SetType { type_id: 1 }],
    }];
    let mut current = life_world(8, 8, &[]);
    let mut replay = current.clone();
    let mut rng = StdRng::seed_from_u64(42);
    let mut replay_rng = StdRng::seed_from_u64(42);

    for _ in 0..4 {
        current = step(&automaton, &current, &mut rng).unwrap();
        replay = step(&automaton, &replay, &mut replay_rng).unwrap();

        assert_eq!(type_grid(&current), type_grid(&replay));
    }
    assert!(current.cells.iter().flatten().any(|cell| cell.type_id == 1));
    assert!(current.cells.iter().flatten().any(|cell| cell.type_id == 0));
}

#[test]
fn rejects_invalid_worlds_and_definitions_before_advancing() {
    let automaton = game_of_life();
    let mut unknown_type = life_world(2, 2, &[]);
    unknown_type.cells[1][1].type_id = 99;
    let mut ragged = life_world(2, 2, &[]);
    ragged.cells[1].pop();
    let mut invalid_agent = life_world(2, 2, &[]);
    invalid_agent.agents.push(Agent {
        id: 7,
        position: Position { row: 2, col: 0 },
        state: state(0),
    });
    let mut rng = StdRng::seed_from_u64(7);

    for invalid in [life_world(0, 0, &[]), ragged, unknown_type, invalid_agent] {
        assert!(step(&automaton, &invalid, &mut rng).is_err());
    }

    let mut invalid_automaton = automaton;
    invalid_automaton.rules[0].probability = Expression::Constant(f64::NAN);
    assert!(step(&invalid_automaton, &life_world(2, 2, &[]), &mut rng).is_err());
}

#[test]
fn reports_a_cell_error_without_partially_updating_the_input_world() {
    let automaton = numeric_automaton(vec![set_property(
        RuleTarget::Cell,
        "value",
        Expression::Divide(
            Box::new(Expression::Constant(1.0)),
            Box::new(Expression::Subtract(
                Box::new(Expression::Constant(3.0)),
                Box::new(Expression::Property("value".to_owned())),
            )),
        ),
    )]);
    let current = numeric_world();
    let mut rng = StdRng::seed_from_u64(7);

    let error = step(&automaton, &current, &mut rng).unwrap_err();

    assert!(error.contains("Cell (0, 2)"));
    assert!(error.contains("Rule 0"));
    assert!(error.contains("Division by zero"));
    assert_eq!(current.cells[0][0].properties["value"], 1.0);
    assert_eq!(current.cells[0][1].properties["value"], 2.0);
    assert_eq!(current.cells[0][2].properties["value"], 3.0);
    assert_eq!(current.agents[0].state.properties["energy"], 12.0);
}

#[test]
fn reports_an_agent_error_without_publishing_the_new_cell_states() {
    let automaton = numeric_automaton(vec![
        set_property(RuleTarget::Cell, "value", Expression::Constant(0.0)),
        set_property(
            RuleTarget::Agent,
            "energy",
            Expression::Divide(
                Box::new(Expression::Constant(1.0)),
                Box::new(Expression::Subtract(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(12.0)),
                )),
            ),
        ),
    ]);
    let current = numeric_world();
    let mut rng = StdRng::seed_from_u64(7);

    let error = step(&automaton, &current, &mut rng).unwrap_err();

    assert!(error.contains("Agent 7"));
    assert!(error.contains("Rule 1"));
    assert!(error.contains("Division by zero"));
    assert_eq!(current.cells[0][0].properties["value"], 1.0);
    assert_eq!(current.cells[0][1].properties["value"], 2.0);
    assert_eq!(current.cells[0][2].properties["value"], 3.0);
    assert_eq!(current.agents[0].state.properties["energy"], 12.0);
}

#[test]
fn preserves_cells_and_agents_when_no_rules_are_defined() {
    let automaton = numeric_automaton(Vec::new());
    let current = numeric_world();
    let mut rng = StdRng::seed_from_u64(7);

    let next = step(&automaton, &current, &mut rng).unwrap();

    assert_eq!(next.height(), current.height());
    assert_eq!(next.width(), current.width());
    for (old, new) in current
        .cells
        .iter()
        .flatten()
        .zip(next.cells.iter().flatten())
    {
        assert_eq!(new.type_id, old.type_id);
        assert_eq!(new.properties, old.properties);
    }
    assert_eq!(next.agents.len(), current.agents.len());
    for (old, new) in current.agents.iter().zip(&next.agents) {
        assert_eq!(new.id, old.id);
        assert_eq!(new.position, old.position);
        assert_eq!(new.state.type_id, old.state.type_id);
        assert_eq!(new.state.properties, old.state.properties);
    }
}

#[test]
fn running_zero_steps_returns_only_the_initial_generation() {
    let automaton = numeric_automaton(Vec::new());
    let initial = numeric_world();

    for save_history in [true, false] {
        let history = run(&automaton, &initial, 0, 42, save_history).unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].generation, 0);
        assert_eq!(
            serde_json::to_value(&history[0].world).unwrap(),
            serde_json::to_value(&initial).unwrap()
        );
    }
}

#[test]
fn records_each_game_of_life_generation_in_order_including_the_initial_one() {
    let automaton = game_of_life();
    let horizontal = life_world(5, 5, &[(2, 1), (2, 2), (2, 3)]);
    let vertical = life_world(5, 5, &[(1, 2), (2, 2), (3, 2)]);

    let history = run(&automaton, &horizontal, 3, 42, true).unwrap();

    assert_eq!(history.len(), 4);
    for (index, snapshot) in history.iter().enumerate() {
        assert_eq!(snapshot.generation, index);
    }
    assert_eq!(type_grid(&history[0].world), type_grid(&horizontal));
    assert_eq!(type_grid(&history[1].world), type_grid(&vertical));
    assert_eq!(type_grid(&history[2].world), type_grid(&horizontal));
    assert_eq!(type_grid(&history[3].world), type_grid(&vertical));
    assert_eq!(horizontal.cells[2][1].type_id, 1);
    assert_eq!(horizontal.cells[1][2].type_id, 0);
}

#[test]
fn records_all_requested_steps_even_when_the_world_stays_the_same() {
    let automaton = game_of_life();
    let initial = life_world(5, 5, &[(1, 1), (1, 2), (2, 1), (2, 2)]);

    let history = run(&automaton, &initial, 5, 42, true).unwrap();

    assert_eq!(history.len(), 6);
    for generation in &history {
        assert_eq!(type_grid(&generation.world), type_grid(&initial));
    }
}

#[test]
fn keeps_independent_snapshots_of_cells_and_agents() {
    let automaton = numeric_automaton(vec![
        set_property(
            RuleTarget::Cell,
            "value",
            Expression::Add(
                Box::new(Expression::Property("value".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
        set_property(
            RuleTarget::Agent,
            "energy",
            Expression::Subtract(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        ),
    ]);
    let initial = numeric_world();

    let mut history = run(&automaton, &initial, 3, 42, true).unwrap();

    assert_eq!(history.len(), 4);
    for (generation, world) in history.iter().enumerate() {
        for (col, cell) in world.world.cells[0].iter().enumerate() {
            assert_eq!(cell.type_id, 0);
            assert_eq!(cell.properties["value"], (col + 1 + generation) as f64);
        }
        for (old, agent) in initial.agents.iter().zip(&world.world.agents) {
            assert_eq!(agent.id, old.id);
            assert_eq!(agent.position, old.position);
            assert_eq!(agent.state.type_id, 1);
            assert_eq!(agent.state.properties["energy"], 12.0 - generation as f64);
        }
    }

    history[1].world.cells[0][0]
        .properties
        .insert("value".to_owned(), 99.0);
    history[1].world.agents[0]
        .state
        .properties
        .insert("energy".to_owned(), 77.0);

    assert_eq!(initial.cells[0][0].properties["value"], 1.0);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
    assert_eq!(history[0].world.cells[0][0].properties["value"], 1.0);
    assert_eq!(history[0].world.agents[0].state.properties["energy"], 12.0);
    assert_eq!(history[2].world.cells[0][0].properties["value"], 3.0);
    assert_eq!(history[2].world.agents[0].state.properties["energy"], 10.0);
}

#[test]
fn replays_probabilistic_history_and_continues_the_random_sequence_between_steps() {
    let mut increment = set_property(
        RuleTarget::Cell,
        "value",
        Expression::Add(
            Box::new(Expression::Property("value".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    );
    increment.probability = Expression::Constant(0.5);
    let automaton = numeric_automaton(vec![increment]);
    let initial = World {
        cells: vec![vec![numeric_state(0, "value", 0.0)]],
        agents: Vec::new(),
    };

    let history = run(&automaton, &initial, 32, 42, true).unwrap();
    let replay = run(&automaton, &initial, 32, 42, true).unwrap();

    assert_eq!(history.len(), 33);
    assert_eq!(replay.len(), history.len());
    for (world, replay_world) in history.iter().zip(&replay) {
        assert_eq!(
            world.world.cells[0][0].properties,
            replay_world.world.cells[0][0].properties
        );
    }

    let changes: Vec<f64> = history
        .windows(2)
        .map(|pair| {
            pair[1].world.cells[0][0].properties["value"]
                - pair[0].world.cells[0][0].properties["value"]
        })
        .collect();
    assert!(
        changes
            .iter()
            .all(|change| *change == 0.0 || *change == 1.0)
    );
    assert!(changes.contains(&0.0));
    assert!(changes.contains(&1.0));
}

#[test]
fn reports_the_generation_where_a_run_fails_without_changing_the_initial_world() {
    let automaton = numeric_automaton(vec![set_property(
        RuleTarget::Cell,
        "value",
        Expression::Add(
            Box::new(Expression::Property("value".to_owned())),
            Box::new(Expression::Constant(50.0)),
        ),
    )]);
    let initial = numeric_world();

    for save_history in [true, false] {
        let error = run(&automaton, &initial, 3, 42, save_history).unwrap_err();

        assert!(error.contains("Generation 2"));
        assert!(error.contains("Cell (0, 0)"));
        assert!(error.contains("Rule 0"));
        assert!(error.contains("value"));
    }
    assert_eq!(initial.cells[0][0].properties["value"], 1.0);
    assert_eq!(initial.cells[0][1].properties["value"], 2.0);
    assert_eq!(initial.cells[0][2].properties["value"], 3.0);
    assert_eq!(initial.agents[0].state.properties["energy"], 12.0);
}

#[test]
fn validates_runs_even_when_zero_generations_are_requested() {
    let automaton = game_of_life();
    let mut invalid_world = life_world(2, 2, &[]);
    invalid_world.cells[1][1].type_id = 99;
    let mut invalid_automaton = automaton.clone();
    invalid_automaton.rules[0].probability = Expression::Constant(f64::NAN);

    for generations in [0, 2] {
        for save_history in [true, false] {
            let world_error =
                run(&automaton, &invalid_world, generations, 42, save_history).unwrap_err();
            assert!(world_error.contains("Generation 0"));
            assert!(world_error.contains("Unknown object type id 99"));

            let definition_error = run(
                &invalid_automaton,
                &life_world(2, 2, &[]),
                generations,
                42,
                save_history,
            )
            .unwrap_err();
            assert!(definition_error.contains("Generation 0"));
            assert!(definition_error.contains("probability"));
        }
    }
}

#[test]
fn recalculates_probability_for_each_generation_before_applying_actions() {
    let mut increment = set_property(RuleTarget::Cell, "value", Expression::Constant(1.0));
    increment.probability = Expression::Subtract(
        Box::new(Expression::Constant(1.0)),
        Box::new(Expression::Property("value".to_owned())),
    );
    let automaton = numeric_automaton(vec![increment]);
    let initial = World {
        cells: vec![vec![numeric_state(0, "value", 0.0)]],
        agents: Vec::new(),
    };

    let history = run(&automaton, &initial, 3, 42, true).unwrap();

    assert_eq!(history.len(), 4);
    assert_eq!(history[0].world.cells[0][0].properties["value"], 0.0);
    for world in &history[1..] {
        assert_eq!(world.world.cells[0][0].properties["value"], 1.0);
    }
    assert_eq!(initial.cells[0][0].properties["value"], 0.0);
}

#[test]
fn final_only_runs_return_the_actual_last_generation_with_its_number() {
    let automaton = game_of_life();
    let initial = life_world(5, 5, &[(2, 1), (2, 2), (2, 3)]);
    let full = run(&automaton, &initial, 3, 42, true).unwrap();
    let final_only = run(&automaton, &initial, 3, 42, false).unwrap();

    assert_eq!(final_only.len(), 1);
    assert_eq!(final_only[0].generation, 3);
    assert_eq!(
        type_grid(&final_only[0].world),
        vec![
            vec![0, 0, 0, 0, 0],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 0, 0, 0],
        ]
    );
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&full[3]).unwrap()
    );
}

#[test]
fn final_only_runs_still_limit_each_intermediate_world_when_agents_are_born() {
    let mut automaton = numeric_automaton(vec![Rule {
        target: RuleTarget::Cell,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![Action::SpawnAgent {
            type_id: 1,
            row_offset: Expression::Constant(0.0),
            col_offset: Expression::Constant(0.0),
            properties: BTreeMap::new(),
        }],
    }]);
    let mut large_type = numeric_type(2, "large", "unused");
    let property = large_type.properties["unused"].clone();
    large_type.properties = (0..5_000)
        .map(|index| (format!("property_{index}"), property.clone()))
        .collect();
    automaton.object_types.push(large_type);
    let initial = World {
        cells: vec![vec![numeric_state(0, "value", 0.0); 10]],
        agents: Vec::new(),
    };

    let error = run(&automaton, &initial, 2, 42, false).unwrap_err();

    assert!(
        error.contains("Generation 1: Simulation history exceeds 100000"),
        "{error}"
    );
    assert!(initial.agents.is_empty());
}
