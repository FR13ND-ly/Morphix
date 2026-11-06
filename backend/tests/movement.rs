use std::collections::BTreeMap;

use backend::{
    models::{
        action::{Action, NeighborSelection},
        agent::Agent,
        automaton::{AutomatonDefinition, Boundary},
        condition::{Comparison, Condition},
        expression::Expression,
        neighborhood::Neighborhood,
        object_type::{ObjectType, PropertyDefinition},
        position::Position,
        rule::{Rule, RuleTarget},
        state::ObjectState,
        world::World,
    },
    simulator::{run, step},
};
use rand::{SeedableRng, rngs::StdRng};

fn neighbor_movement(selection: NeighborSelection, cell_type: Option<u8>) -> Action {
    Action::MoveToFreeNeighbor {
        selection,
        cell_type,
        offsets: None,
    }
}

fn neighbor_definition(boundary: Boundary, selection: NeighborSelection) -> AutomatonDefinition {
    let mut result = definition(boundary);
    result.rules[0].actions = vec![neighbor_movement(selection, Some(0))];
    result
}

#[test]
fn first_free_movement_uses_old_occupancy_even_when_another_agent_vacates_a_position() {
    let definition =
        neighbor_definition(Boundary::Fixed { state: floor() }, NeighborSelection::First);
    let initial = world(
        1,
        5,
        vec![agent(7, (0, 2), (0.0, 0.0)), agent(8, (0, 1), (0.0, 0.0))],
    );
    let next = advance(&definition, &initial);
    assert_eq!(next.agents[0].position.col, 3);
    assert_eq!(next.agents[1].position.col, 0);
    assert_eq!(initial.agents[0].position.col, 2);
    assert_eq!(initial.agents[1].position.col, 1);
}

#[test]
fn free_movement_filters_target_terrain_when_requested() {
    let mut initial = world(1, 3, vec![agent(7, (0, 1), (0.0, 0.0))]);
    initial.cells[0][2] = ObjectState {
        type_id: 2,
        properties: BTreeMap::new(),
    };
    for (cell_type, expected) in [(Some(0), 0), (None, 2)] {
        let mut definition =
            neighbor_definition(Boundary::Fixed { state: floor() }, NeighborSelection::First);
        definition.neighborhood.offsets = vec![(0, 1), (0, -1)];
        definition.object_types.push(ObjectType {
            id: 2,
            name: "wall".to_owned(),
            properties: BTreeMap::new(),
        });
        definition.rules[0].actions = vec![neighbor_movement(NeighborSelection::First, cell_type)];
        let next = advance(&definition, &initial);
        assert_eq!(next.agents[0].position.col, expected);
    }
}

#[test]
fn conflicting_free_neighbor_choices_cancel_moves_but_keep_state_changes() {
    for selection in [NeighborSelection::First, NeighborSelection::Random] {
        let mut definition = neighbor_definition(Boundary::Fixed { state: floor() }, selection);
        definition.rules[0].actions.insert(
            0,
            Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::Subtract(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(1.0)),
                ),
            },
        );
        let initial = world(
            1,
            3,
            vec![agent(7, (0, 0), (0.0, 0.0)), agent(8, (0, 2), (0.0, 0.0))],
        );
        let next = advance(&definition, &initial);
        assert_eq!(positions(&next), positions(&initial));
        for agent in &next.agents {
            assert_eq!(agent.state.properties["energy"], 9.0);
        }
    }
}

#[test]
fn random_walks_replay_with_the_same_seed_and_agree_with_final_only_runs() {
    let definition = neighbor_definition(Boundary::Wrap, NeighborSelection::Random);
    let initial = world(1, 5, vec![agent(7, (0, 2), (0.0, 0.0))]);
    let history = run(&definition, &initial, 64, 42, true).unwrap();
    let replay = run(&definition, &initial, 64, 42, true).unwrap();
    let final_only = run(&definition, &initial, 64, 42, false).unwrap();
    let other_seed = run(&definition, &initial, 64, 43, true).unwrap();
    assert_eq!(
        serde_json::to_value(&history).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!(final_only.len(), 1);
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&history[64]).unwrap()
    );
    assert!(
        history
            .iter()
            .zip(&other_seed)
            .any(|(first, second)| positions(&first.world) != positions(&second.world))
    );
    for pair in history.windows(2) {
        let previous = pair[0].world.agents[0].position.col;
        let next = pair[1].world.agents[0].position.col;
        assert!(next == (previous + 1) % 5 || next == (previous + 4) % 5);
    }
}

#[test]
fn tiny_worlds_handle_zero_or_one_distinct_free_destination() {
    for boundary in [Boundary::Wrap, Boundary::Fixed { state: floor() }] {
        for selection in [NeighborSelection::First, NeighborSelection::Random] {
            let definition = neighbor_definition(boundary.clone(), selection);
            let one = world(1, 1, vec![agent(7, (0, 0), (0.0, 0.0))]);
            assert_eq!(positions(&advance(&definition, &one)), positions(&one));
            let two = world(1, 2, vec![agent(7, (0, 0), (0.0, 0.0))]);
            let history = run(&definition, &two, 3, 42, true).unwrap();
            for (generation, snapshot) in history.iter().enumerate() {
                assert_eq!(snapshot.world.agents[0].position.col, generation % 2);
            }
        }
    }
}

#[test]
fn no_free_destination_keeps_the_selected_rule_without_trying_a_later_rule() {
    let mut definition = neighbor_definition(Boundary::Wrap, NeighborSelection::Random);
    definition.rules[0].actions.insert(
        0,
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(9.0),
        },
    );
    let mut fallback = definition.rules[0].clone();
    fallback.actions = vec![Action::SetProperty {
        property: "energy".to_owned(),
        value: Expression::Constant(99.0),
    }];
    definition.rules.push(fallback);
    let initial = world(1, 1, vec![agent(7, (0, 0), (0.0, 0.0))]);
    let next = advance(&definition, &initial);
    assert_eq!(next.agents[0].state.properties["energy"], 9.0);
    assert_eq!(next.agents[0].position.col, 0);
}

#[test]
fn positions_freed_by_removal_become_candidates_on_the_next_generation() {
    let mut definition =
        neighbor_definition(Boundary::Fixed { state: floor() }, NeighborSelection::First);
    definition.rules.insert(0, removal_rule());
    let initial = world(
        1,
        3,
        vec![
            agent(7, (0, 1), (0.0, 0.0)),
            exhausted(8, (0, 0)),
            agent(9, (0, 2), (0.0, 0.0)),
        ],
    );
    let history = run(&definition, &initial, 2, 42, true).unwrap();
    assert_eq!(history[1].world.agents.len(), 2);
    assert_eq!(history[1].world.agents[0].position.col, 1);
    assert_eq!(history[2].world.agents[0].position.col, 0);
    assert_eq!(history[2].world.agents[1].position.col, 2);
}

fn floor() -> ObjectState {
    ObjectState {
        type_id: 0,
        properties: BTreeMap::new(),
    }
}

fn movement() -> Action {
    Action::Move {
        row_offset: Expression::Property("dr".to_owned()),
        col_offset: Expression::Property("dc".to_owned()),
    }
}

fn constant_move(row: f64, col: f64) -> Action {
    Action::Move {
        row_offset: Expression::Constant(row),
        col_offset: Expression::Constant(col),
    }
}

fn definition(boundary: Boundary) -> AutomatonDefinition {
    let offset = PropertyDefinition {
        default_value: 0.0,
        min_value: f64::from(i32::MIN),
        max_value: f64::from(i32::MAX),
        integer_only: false,
    };
    AutomatonDefinition {
        name: "Moving agents".to_owned(),
        object_types: vec![
            ObjectType {
                id: 0,
                name: "floor".to_owned(),
                properties: BTreeMap::new(),
            },
            ObjectType {
                id: 1,
                name: "walker".to_owned(),
                properties: BTreeMap::from([
                    ("dr".to_owned(), offset.clone()),
                    ("dc".to_owned(), offset),
                    (
                        "energy".to_owned(),
                        PropertyDefinition {
                            default_value: 10.0,
                            min_value: 0.0,
                            max_value: 100.0,
                            integer_only: false,
                        },
                    ),
                ]),
            },
        ],
        neighborhood: Neighborhood {
            offsets: vec![(0, -1), (0, 1)],
        },
        boundary,
        rules: vec![Rule {
            target: RuleTarget::Agent,
            condition: Condition::TypeIs(1),
            probability: Expression::Constant(1.0),
            actions: vec![movement()],
        }],
    }
}

fn agent(id: u64, position: (usize, usize), offset: (f64, f64)) -> Agent {
    Agent {
        id,
        position: Position {
            row: position.0,
            col: position.1,
        },
        state: ObjectState {
            type_id: 1,
            properties: BTreeMap::from([
                ("dr".to_owned(), offset.0),
                ("dc".to_owned(), offset.1),
                ("energy".to_owned(), 10.0),
            ]),
        },
    }
}

fn world(height: usize, width: usize, agents: Vec<Agent>) -> World {
    World {
        cells: vec![vec![floor(); width]; height],
        agents,
    }
}

fn positions(world: &World) -> BTreeMap<u64, Position> {
    world
        .agents
        .iter()
        .map(|agent| (agent.id, agent.position))
        .collect()
}

fn advance(definition: &AutomatonDefinition, world: &World) -> World {
    let mut rng = StdRng::seed_from_u64(42);
    let next = step(definition, world, &mut rng).unwrap();
    definition.validate_world(&next).unwrap();
    next
}

fn removal_rule() -> Rule {
    Rule {
        target: RuleTarget::Agent,
        condition: Condition::All(vec![
            Condition::TypeIs(1),
            Condition::Compare {
                left: Expression::Property("energy".to_owned()),
                operator: Comparison::LessOrEqual,
                right: Expression::Constant(0.0),
            },
        ]),
        probability: Expression::Constant(1.0),
        actions: vec![Action::RemoveAgent {}],
    }
}

fn exhausted(id: u64, position: (usize, usize)) -> Agent {
    let mut agent = agent(id, position, (0.0, 0.0));
    agent.state.properties.insert("energy".to_owned(), 0.0);
    agent
}

#[test]
fn moves_an_agent_without_changing_its_identity_terrain_or_input() {
    let definition = definition(Boundary::Wrap);
    let current = world(4, 5, vec![agent(7, (1, 1), (-1.0, 2.0))]);

    let next = advance(&definition, &current);

    assert_eq!(next.agents[0].id, 7);
    assert_eq!(next.agents[0].position, Position { row: 0, col: 3 });
    assert_eq!(
        next.agents[0].state.properties,
        current.agents[0].state.properties
    );
    assert!(next.cells.iter().flatten().all(|cell| cell.type_id == 0));
    assert_eq!(current.agents[0].position, Position { row: 1, col: 1 });
}

#[test]
fn wraps_signed_offsets_including_offsets_larger_than_the_world() {
    let definition = definition(Boundary::Wrap);
    for (start, offset, expected) in [
        ((0, 0), (-1.0, -1.0), Position { row: 2, col: 4 }),
        ((1, 2), (7.0, -12.0), Position { row: 2, col: 0 }),
        (
            (0, 0),
            (f64::from(i32::MIN), f64::from(i32::MAX)),
            Position { row: 1, col: 2 },
        ),
    ] {
        let next = advance(&definition, &world(3, 5, vec![agent(7, start, offset)]));
        assert_eq!(next.agents[0].position, expected);
    }
}

#[test]
fn keeps_agents_inside_fixed_boundaries_without_clamping_partial_moves() {
    let definition = definition(Boundary::Fixed { state: floor() });
    for (start, offset) in [
        ((0, 0), (-1.0, 0.0)),
        ((2, 4), (1.0, 1.0)),
        ((1, 4), (1.0, 1.0)),
    ] {
        let current = world(3, 5, vec![agent(7, start, offset)]);
        let next = advance(&definition, &current);
        assert_eq!(positions(&next), positions(&current));
    }
}

#[test]
fn cancels_all_moves_to_the_same_destination_without_order_priority() {
    let definition = definition(Boundary::Wrap);
    let mut current = world(
        1,
        3,
        vec![agent(7, (0, 0), (0.0, 1.0)), agent(8, (0, 2), (0.0, -1.0))],
    );
    let expected = positions(&current);

    assert_eq!(positions(&advance(&definition, &current)), expected);
    current.agents.reverse();
    assert_eq!(positions(&advance(&definition, &current)), expected);
}

#[test]
fn blocks_entry_into_a_stationary_agents_cell_while_other_moves_succeed() {
    let definition = definition(Boundary::Wrap);
    let current = world(
        1,
        4,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 1), (0.0, 0.0)),
            agent(9, (0, 2), (0.0, 1.0)),
        ],
    );

    let next = advance(&definition, &current);

    assert_eq!(next.agents[0].position.col, 0);
    assert_eq!(next.agents[1].position.col, 1);
    assert_eq!(next.agents[2].position.col, 3);
}

#[test]
fn allows_swaps_and_closed_cycles_when_every_agent_vacates_its_cell() {
    let definition = definition(Boundary::Wrap);
    for agents in [
        vec![agent(7, (0, 0), (0.0, 1.0)), agent(8, (0, 1), (0.0, -1.0))],
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 1), (0.0, 1.0)),
            agent(9, (0, 2), (0.0, -2.0)),
        ],
    ] {
        let current = world(1, 3, agents);
        let next = advance(&definition, &current);
        assert_eq!(next.agents[0].position.col, 1);
        assert_eq!(
            next.agents[1].position.col,
            if next.agents.len() == 2 { 0 } else { 2 }
        );
        if next.agents.len() == 3 {
            assert_eq!(next.agents[2].position.col, 0);
        }
    }
}

#[test]
fn allows_a_chain_to_advance_into_a_cell_that_becomes_free() {
    let definition = definition(Boundary::Fixed { state: floor() });
    let current = world(
        1,
        4,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 1), (0.0, 1.0)),
            agent(9, (0, 2), (0.0, 1.0)),
        ],
    );

    let next = advance(&definition, &current);

    assert_eq!(
        next.agents
            .iter()
            .map(|agent| agent.position.col)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn propagates_a_stationary_blockage_through_the_entire_chain() {
    let definition = definition(Boundary::Wrap);
    let mut current = world(
        1,
        4,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 1), (0.0, 1.0)),
            agent(9, (0, 2), (0.0, 0.0)),
        ],
    );
    let expected = positions(&current);

    assert_eq!(positions(&advance(&definition, &current)), expected);
    current.agents.reverse();
    assert_eq!(positions(&advance(&definition, &current)), expected);
}

#[test]
fn propagates_cancelled_destination_conflicts_to_other_dependent_moves() {
    let definition = definition(Boundary::Wrap);
    let mut current = world(
        1,
        5,
        vec![
            agent(7, (0, 0), (0.0, 2.0)),
            agent(8, (0, 4), (0.0, -2.0)),
            agent(9, (0, 1), (0.0, -1.0)),
            agent(10, (0, 3), (0.0, -2.0)),
        ],
    );
    let expected = positions(&current);

    assert_eq!(positions(&advance(&definition, &current)), expected);
    current.agents.reverse();
    assert_eq!(positions(&advance(&definition, &current)), expected);
}

#[test]
fn permits_entry_into_a_shared_starting_cell_only_when_all_occupants_leave() {
    let definition = definition(Boundary::Wrap);
    let mut current = world(
        1,
        4,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 0), (0.0, 0.0)),
            agent(9, (0, 3), (0.0, -3.0)),
        ],
    );
    let blocked = advance(&definition, &current);
    assert_eq!(blocked.agents[0].position.col, 1);
    assert_eq!(blocked.agents[1].position.col, 0);
    assert_eq!(blocked.agents[2].position.col, 3);

    current.agents[1]
        .state
        .properties
        .insert("dc".to_owned(), 2.0);
    let released = advance(&definition, &current);
    assert_eq!(released.agents[0].position.col, 1);
    assert_eq!(released.agents[1].position.col, 2);
    assert_eq!(released.agents[2].position.col, 0);
}

#[test]
fn evaluates_movement_from_the_old_state_and_keeps_state_changes_when_moves_conflict() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions = vec![
        Action::SetProperty {
            property: "dc".to_owned(),
            value: Expression::Constant(2.0),
        },
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Subtract(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        },
        movement(),
    ];
    let current = world(
        1,
        4,
        vec![agent(7, (0, 0), (0.0, 1.0)), agent(8, (0, 2), (0.0, -1.0))],
    );

    let next = advance(&definition, &current);

    assert_eq!(positions(&next), positions(&current));
    for agent in &next.agents {
        assert_eq!(agent.state.properties["dc"], 2.0);
        assert_eq!(agent.state.properties["energy"], 9.0);
    }
    assert_eq!(current.agents[0].state.properties["dc"], 1.0);
    assert_eq!(current.agents[1].state.properties["dc"], -1.0);
    assert_eq!(current.agents[0].state.properties["energy"], 10.0);
}

#[test]
fn validates_movement_target_count_and_constant_offsets_before_a_zero_step_run() {
    let initial = world(1, 4, Vec::new());
    let mut cell_rule = definition(Boundary::Wrap);
    cell_rule.rules[0].target = RuleTarget::Cell;
    let error = run(&cell_rule, &initial, 0, 42, true).unwrap_err();
    assert!(
        error.contains("Generation 0: Rule 0: Action 0: Move is only allowed in agent rules"),
        "{error}"
    );

    let mut duplicate = definition(Boundary::Wrap);
    duplicate.rules[0].actions.push(movement());
    let error = run(&duplicate, &initial, 0, 42, true).unwrap_err();
    assert!(
        error.contains("Action 1: A rule may contain at most one movement action"),
        "{error}"
    );

    for invalid in [
        0.5,
        f64::from(i32::MAX) + 1.0,
        f64::from(i32::MIN) - 1.0,
        f64::INFINITY,
        f64::NAN,
    ] {
        let mut definition = definition(Boundary::Wrap);
        definition.rules[0].actions = vec![constant_move(0.0, invalid)];
        let error = run(&definition, &initial, 0, 42, true).unwrap_err();
        assert!(
            error.contains("Generation 0: Rule 0: Action 0: col_offset:"),
            "{error}"
        );
    }
}

#[test]
fn reports_invalid_calculated_offsets_without_partially_updating_the_input() {
    let definition = definition(Boundary::Wrap);
    let initial = world(
        2,
        4,
        vec![agent(7, (0, 0), (0.0, 1.0)), agent(9, (0, 2), (0.5, 0.0))],
    );

    let error = run(&definition, &initial, 1, 42, true).unwrap_err();

    assert!(
        error.contains("Generation 1: Agent 9: Rule 0: row_offset:"),
        "{error}"
    );
    assert_eq!(initial.agents[0].position.col, 0);
    assert_eq!(initial.agents[1].position.col, 2);

    let mut overflow = definition;
    overflow.rules[0].actions = vec![Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Multiply(
            Box::new(Expression::Property("dc".to_owned())),
            Box::new(Expression::Constant(2.0)),
        ),
    }];
    let initial = world(1, 4, vec![agent(7, (0, 0), (0.0, f64::from(i32::MAX)))]);
    let error = run(&overflow, &initial, 1, 42, true).unwrap_err();
    assert!(
        error.contains("Generation 1: Agent 7: Rule 0: col_offset:"),
        "{error}"
    );
    assert!(error.contains("i32 range"), "{error}");
}

#[test]
fn a_failed_probability_check_produces_no_movement_and_does_not_try_later_rules() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules.push(definition.rules[0].clone());
    definition.rules[0].probability = Expression::Constant(0.0);
    let initial = world(1, 4, vec![agent(7, (0, 0), (0.0, 1.0))]);

    let next = advance(&definition, &initial);

    assert_eq!(positions(&next), positions(&initial));
}

#[test]
fn reproduces_probabilistic_movement_across_a_sequence_of_generations() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].probability = Expression::Constant(0.5);
    let initial = world(1, 40, vec![agent(7, (0, 0), (0.0, 1.0))]);

    let history = run(&definition, &initial, 32, 42, true).unwrap();
    let replay = run(&definition, &initial, 32, 42, true).unwrap();
    let final_only = run(&definition, &initial, 32, 42, false).unwrap();

    assert_eq!(history.len(), 33);
    assert_eq!(final_only.len(), 1);
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&history[32]).unwrap()
    );
    for (world, replay_world) in history.iter().zip(&replay) {
        assert_eq!(positions(&world.world), positions(&replay_world.world));
    }
    let changes: Vec<usize> = history
        .windows(2)
        .map(|pair| pair[1].world.agents[0].position.col - pair[0].world.agents[0].position.col)
        .collect();
    assert!(changes.contains(&0));
    assert!(changes.contains(&1));
    assert!(changes.iter().all(|change| *change <= 1));
}

#[test]
fn uses_the_agents_current_state_to_control_movement_probability() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].probability = Expression::Divide(
        Box::new(Expression::Property("energy".to_owned())),
        Box::new(Expression::Constant(10.0)),
    );
    definition.rules[0].actions.insert(
        0,
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        },
    );
    let initial = world(1, 4, vec![agent(7, (0, 0), (0.0, 1.0))]);

    let history = run(&definition, &initial, 3, 42, true).unwrap();

    assert_eq!(history[0].world.agents[0].position.col, 0);
    for world in &history[1..] {
        assert_eq!(world.world.agents[0].position.col, 1);
        assert_eq!(world.world.agents[0].state.properties["energy"], 0.0);
    }
    assert_eq!(initial.agents[0].state.properties["energy"], 10.0);
}

#[test]
fn removes_only_matching_agents_without_changing_the_input_or_terrain() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules.insert(0, removal_rule());
    let initial = world(
        1,
        4,
        vec![exhausted(7, (0, 0)), agent(8, (0, 2), (0.0, 1.0))],
    );

    let next = advance(&definition, &initial);

    assert_eq!(next.agents.len(), 1);
    assert_eq!(next.agents[0].id, 8);
    assert_eq!(next.agents[0].position, Position { row: 0, col: 3 });
    assert_eq!(next.agents[0].state.properties["energy"], 10.0);
    assert!(next.cells.iter().flatten().all(|cell| cell.type_id == 0));
    assert_eq!(initial.agents.len(), 2);
    assert_eq!(initial.agents[0].id, 7);
    assert_eq!(initial.agents[1].position.col, 2);
}

#[test]
fn removes_on_the_step_after_energy_reaches_zero_and_preserves_history() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions.insert(
        0,
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Subtract(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        },
    );
    definition.rules.insert(0, removal_rule());
    let mut initial = world(1, 4, vec![agent(7, (0, 0), (0.0, 1.0))]);
    initial.agents[0]
        .state
        .properties
        .insert("energy".to_owned(), 1.0);

    let history = run(&definition, &initial, 3, 42, true).unwrap();

    assert_eq!(history.len(), 4);
    assert_eq!(history[0].world.agents[0].state.properties["energy"], 1.0);
    assert_eq!(history[0].world.agents[0].position.col, 0);
    assert_eq!(history[1].world.agents[0].state.properties["energy"], 0.0);
    assert_eq!(history[1].world.agents[0].position.col, 1);
    assert!(history[2].world.agents.is_empty());
    assert!(history[3].world.agents.is_empty());
    assert_eq!(initial.agents[0].state.properties["energy"], 1.0);
}

#[test]
fn removal_frees_a_position_for_a_chain_in_the_same_step_in_either_list_order() {
    let mut definition = definition(Boundary::Fixed { state: floor() });
    definition.rules.insert(0, removal_rule());
    let mut initial = world(
        1,
        3,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 1), (0.0, 1.0)),
            exhausted(9, (0, 2)),
        ],
    );
    let expected = BTreeMap::from([
        (7, Position { row: 0, col: 1 }),
        (8, Position { row: 0, col: 2 }),
    ]);

    assert_eq!(positions(&advance(&definition, &initial)), expected);
    initial.agents.reverse();
    assert_eq!(positions(&advance(&definition, &initial)), expected);
}

#[test]
fn a_surviving_occupant_still_blocks_a_shared_position_after_removal() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules.insert(0, removal_rule());
    let mut initial = world(
        1,
        3,
        vec![
            exhausted(7, (0, 1)),
            agent(8, (0, 1), (0.0, 0.0)),
            agent(9, (0, 0), (0.0, 1.0)),
        ],
    );

    let blocked = advance(&definition, &initial);
    assert_eq!(
        positions(&blocked),
        BTreeMap::from([
            (8, Position { row: 0, col: 1 }),
            (9, Position { row: 0, col: 0 }),
        ])
    );

    initial.agents[1]
        .state
        .properties
        .insert("dc".to_owned(), 1.0);
    let released = advance(&definition, &initial);
    assert_eq!(
        positions(&released),
        BTreeMap::from([
            (8, Position { row: 0, col: 2 }),
            (9, Position { row: 0, col: 1 }),
        ])
    );
}

#[test]
fn multiple_moves_into_a_removed_agents_position_still_conflict() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules.insert(0, removal_rule());
    let mut initial = world(
        1,
        3,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            exhausted(8, (0, 1)),
            agent(9, (0, 2), (0.0, -1.0)),
        ],
    );
    let expected = BTreeMap::from([
        (7, Position { row: 0, col: 0 }),
        (9, Position { row: 0, col: 2 }),
    ]);

    assert_eq!(positions(&advance(&definition, &initial)), expected);
    initial.agents.reverse();
    assert_eq!(positions(&advance(&definition, &initial)), expected);
}

#[test]
fn allows_all_agents_to_disappear_and_continues_with_an_empty_agent_list() {
    for boundary in [Boundary::Wrap, Boundary::Fixed { state: floor() }] {
        let mut definition = definition(boundary);
        definition.rules.insert(0, removal_rule());
        let initial = world(1, 2, vec![exhausted(7, (0, 0)), exhausted(8, (0, 1))]);

        let history = run(&definition, &initial, 3, 42, true).unwrap();

        assert_eq!(history[0].world.agents.len(), 2);
        for snapshot in &history[1..] {
            assert!(snapshot.world.agents.is_empty());
            definition.validate_world(&snapshot.world).unwrap();
        }
    }
}

#[test]
fn validates_removal_even_in_skipped_rules_and_zero_step_runs() {
    let initial = world(1, 2, Vec::new());
    let mut cell_definition = definition(Boundary::Wrap);
    let mut cell_rule = removal_rule();
    cell_rule.target = RuleTarget::Cell;
    cell_rule.probability = Expression::Constant(0.0);
    cell_rule.condition = Condition::Any(Vec::new());
    cell_definition.rules = vec![cell_rule];
    let error = run(&cell_definition, &initial, 0, 42, true).unwrap_err();
    assert!(error.contains("Generation 0: Rule 0: Action 0:"), "{error}");
    assert!(
        error.contains("RemoveAgent is only allowed in agent rules"),
        "{error}"
    );

    for other in [
        Action::RemoveAgent {},
        movement(),
        Action::SetType { type_id: 1 },
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        },
    ] {
        for actions in [
            vec![Action::RemoveAgent {}, other.clone()],
            vec![other.clone(), Action::RemoveAgent {}],
        ] {
            let mut definition = definition(Boundary::Wrap);
            let mut rule = removal_rule();
            rule.actions = actions;
            rule.probability = Expression::Constant(0.0);
            rule.condition = Condition::Any(Vec::new());
            definition.rules = vec![rule];

            let error = run(&definition, &initial, 0, 42, true).unwrap_err();
            assert!(error.contains("Generation 0: Rule 0: Action"), "{error}");
            assert!(
                error.contains("RemoveAgent must be the only action"),
                "{error}"
            );
        }
    }
}

#[test]
fn failed_removal_keeps_the_agent_and_does_not_fall_back_to_movement() {
    let mut definition = definition(Boundary::Wrap);
    let mut rule = removal_rule();
    rule.probability = Expression::Constant(0.0);
    definition.rules.insert(0, rule);
    let mut initial = world(1, 3, vec![exhausted(7, (0, 0))]);
    initial.agents[0]
        .state
        .properties
        .insert("dc".to_owned(), 1.0);

    let next = advance(&definition, &initial);

    assert_eq!(positions(&next), positions(&initial));
    assert_eq!(next.agents[0].state.properties["energy"], 0.0);
}

#[test]
fn reproduces_probabilistic_removal_with_the_same_seed() {
    let mut definition = definition(Boundary::Wrap);
    let mut rule = removal_rule();
    rule.probability = Expression::Constant(0.5);
    definition.rules.insert(0, rule);
    let initial = world(
        1,
        32,
        (0..32).map(|col| exhausted(col as u64, (0, col))).collect(),
    );

    let history = run(&definition, &initial, 4, 42, true).unwrap();
    let replay = run(&definition, &initial, 4, 42, true).unwrap();

    assert!(!history[1].world.agents.is_empty());
    assert!(history[1].world.agents.len() < initial.agents.len());
    for (snapshot, replay_snapshot) in history.iter().zip(&replay) {
        assert_eq!(
            positions(&snapshot.world),
            positions(&replay_snapshot.world)
        );
    }
    for pair in history.windows(2) {
        let previous = positions(&pair[0].world);
        assert!(
            pair[1]
                .world
                .agents
                .iter()
                .all(|agent| previous.get(&agent.id) == Some(&agent.position))
        );
    }
}

fn spawn_action(row: f64, col: f64) -> Action {
    Action::SpawnAgent {
        type_id: 1,
        row_offset: Expression::Constant(row),
        col_offset: Expression::Constant(col),
        properties: BTreeMap::new(),
    }
}

#[test]
fn creates_an_agent_from_the_old_parent_state_and_position_alongside_parent_changes() {
    let mut definition = definition(Boundary::Wrap);
    let mut spawn = spawn_action(1.0, 0.0);
    if let Action::SpawnAgent { properties, .. } = &mut spawn {
        properties.insert(
            "energy".to_owned(),
            Expression::Divide(
                Box::new(Expression::Property("energy".to_owned())),
                Box::new(Expression::Constant(2.0)),
            ),
        );
    }
    definition.rules[0].actions = vec![
        movement(),
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(0.0),
        },
        spawn,
    ];
    let initial = world(3, 5, vec![agent(7, (1, 1), (0.0, 1.0))]);

    let next = advance(&definition, &initial);

    assert_eq!(next.agents.len(), 2);
    assert_eq!(next.agents[0].id, 7);
    assert_eq!(next.agents[0].position, Position { row: 1, col: 2 });
    assert_eq!(next.agents[0].state.properties["energy"], 0.0);
    assert_eq!(next.agents[1].id, 8);
    assert_eq!(next.agents[1].position, Position { row: 2, col: 1 });
    assert_eq!(next.agents[1].state.properties["energy"], 5.0);
    assert_eq!(next.agents[1].state.properties["dc"], 0.0);
    assert_eq!(initial.agents.len(), 1);
    assert_eq!(initial.agents[0].state.properties["energy"], 10.0);
    assert_eq!(initial.agents[0].position, Position { row: 1, col: 1 });
}

#[test]
fn cells_can_spawn_agents_that_start_executing_rules_in_the_next_generation() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions = vec![Action::SetProperty {
        property: "energy".to_owned(),
        value: Expression::Subtract(
            Box::new(Expression::Property("energy".to_owned())),
            Box::new(Expression::Constant(1.0)),
        ),
    }];
    definition.rules.insert(
        0,
        Rule {
            target: RuleTarget::Cell,
            condition: Condition::TypeIs(0),
            probability: Expression::Constant(1.0),
            actions: vec![spawn_action(0.0, 0.0)],
        },
    );
    let initial = world(1, 2, Vec::new());

    let history = run(&definition, &initial, 2, 42, true).unwrap();

    assert!(history[0].world.agents.is_empty());
    assert_eq!(history[1].world.agents.len(), 2);
    assert_eq!(history[1].world.agents[0].id, 1);
    assert_eq!(history[1].world.agents[1].id, 2);
    for agent in &history[1].world.agents {
        assert_eq!(agent.state.properties["energy"], 10.0);
    }
    assert_eq!(history[2].world.agents.len(), 2);
    for agent in &history[2].world.agents {
        assert_eq!(agent.state.properties["energy"], 9.0);
    }
}

#[test]
fn spawn_positions_wrap_but_fixed_boundaries_cancel_outside_births() {
    for (row, col, expected) in [
        (-1.0, -1.0, Position { row: 2, col: 4 }),
        (
            f64::from(i32::MIN),
            f64::from(i32::MAX),
            Position { row: 1, col: 2 },
        ),
    ] {
        let mut definition = definition(Boundary::Wrap);
        definition.rules[0].actions = vec![spawn_action(row, col)];
        let next = advance(
            &definition,
            &world(3, 5, vec![agent(7, (0, 0), (0.0, 0.0))]),
        );
        assert_eq!(next.agents.len(), 2);
        assert_eq!(next.agents[1].position, expected);
    }

    for offset in [(-1.0, 0.0), (0.0, -1.0), (0.0, 5.0)] {
        let mut definition = definition(Boundary::Fixed { state: floor() });
        definition.rules[0].actions = vec![spawn_action(offset.0, offset.1)];
        let next = advance(
            &definition,
            &world(3, 5, vec![agent(7, (0, 0), (0.0, 0.0))]),
        );
        assert_eq!(next.agents.len(), 1);
    }
}

#[test]
fn births_use_final_occupancy_and_keep_parent_changes_when_cancelled() {
    let mut definition = definition(Boundary::Wrap);
    let mut reproduction = definition.rules[0].clone();
    reproduction.condition = Condition::Compare {
        left: Expression::Property("energy".to_owned()),
        operator: Comparison::GreaterOrEqual,
        right: Expression::Constant(20.0),
    };
    reproduction.actions = vec![
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(1.0),
        },
        spawn_action(0.0, 1.0),
    ];
    definition.rules.insert(0, reproduction);

    for (other_position, other_offset, expect_birth) in [
        ((0, 1), (0.0, 0.0), false),
        ((0, 1), (0.0, 1.0), true),
        ((0, 2), (0.0, -1.0), false),
    ] {
        let mut initial = world(
            1,
            4,
            vec![
                agent(7, (0, 0), (0.0, 0.0)),
                agent(9, other_position, other_offset),
            ],
        );
        initial.agents[0]
            .state
            .properties
            .insert("energy".to_owned(), 20.0);

        let next = advance(&definition, &initial);

        assert_eq!(next.agents.len(), if expect_birth { 3 } else { 2 });
        assert_eq!(next.agents[0].state.properties["energy"], 1.0);
        if expect_birth {
            assert_eq!(next.agents[2].id, 10);
            assert_eq!(next.agents[2].position, Position { row: 0, col: 1 });
        }
    }
}

#[test]
fn cancels_all_births_requesting_the_same_position_without_blocking_other_births() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions = vec![Action::SpawnAgent {
        type_id: 1,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Property("dc".to_owned()),
        properties: BTreeMap::new(),
    }];
    let mut initial = world(
        1,
        6,
        vec![
            agent(7, (0, 0), (0.0, 1.0)),
            agent(8, (0, 2), (0.0, -1.0)),
            agent(9, (0, 4), (0.0, 1.0)),
        ],
    );
    let mut expected = positions(&initial);
    expected.insert(10, Position { row: 0, col: 5 });

    assert_eq!(positions(&advance(&definition, &initial)), expected);
    initial.agents.reverse();
    assert_eq!(positions(&advance(&definition, &initial)), expected);
}

#[test]
fn removal_frees_birth_positions_and_ids_are_not_reused_across_a_run() {
    let mut definition = definition(Boundary::Wrap);
    let mut spawn = spawn_action(0.0, 1.0);
    if let Action::SpawnAgent { properties, .. } = &mut spawn {
        properties.insert("energy".to_owned(), Expression::Constant(0.0));
    }
    definition.rules[0].actions = vec![spawn];
    definition.rules.insert(0, removal_rule());
    let initial = world(
        1,
        2,
        vec![agent(7, (0, 0), (0.0, 0.0)), exhausted(100, (0, 1))],
    );

    let history = run(&definition, &initial, 3, 42, true).unwrap();

    assert_eq!(history[0].world.agents[1].id, 100);
    for (generation, snapshot) in history.iter().enumerate().skip(1) {
        assert_eq!(snapshot.world.agents.len(), 2);
        assert_eq!(snapshot.world.agents[0].id, 7);
        assert_eq!(snapshot.world.agents[1].id, 100 + generation as u64);
        assert_eq!(snapshot.world.agents[1].state.properties["energy"], 0.0);
        assert_eq!(
            snapshot.world.agents[1].position,
            Position { row: 0, col: 1 }
        );
    }
    let final_only = run(&definition, &initial, 3, 42, false).unwrap();
    assert_eq!(final_only.len(), 1);
    assert_eq!(final_only[0].generation, 3);
    assert_eq!(final_only[0].world.agents[1].id, 103);
    let (mut initial_history, mut checkpoint) =
        backend::simulator::run_with_checkpoint(&definition, &initial, 0, 42, true).unwrap();
    let mut current = initial_history.pop().unwrap();
    for expected in history.iter().skip(1) {
        (current, checkpoint) =
            backend::simulator::continue_step(&definition, &current, 42, checkpoint).unwrap();
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&history[3]).unwrap()
    );
}

#[test]
fn failed_spawn_probability_does_not_try_a_later_movement_rule() {
    let mut definition = definition(Boundary::Wrap);
    let mut spawning = definition.rules[0].clone();
    spawning.actions = vec![spawn_action(0.0, 1.0)];
    spawning.probability = Expression::Constant(0.0);
    definition.rules.insert(0, spawning);
    let initial = world(1, 4, vec![agent(7, (0, 0), (0.0, 1.0))]);

    let next = advance(&definition, &initial);

    assert_eq!(positions(&next), positions(&initial));
}

#[test]
fn reproduces_probabilistic_births_including_ids_states_and_history() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions = vec![spawn_action(0.0, 1.0)];
    definition.rules[0].probability = Expression::Constant(0.5);
    let initial = world(
        1,
        64,
        (0..32)
            .map(|index| agent(index as u64, (0, index * 2), (0.0, 0.0)))
            .collect(),
    );

    let history = run(&definition, &initial, 3, 42, true).unwrap();
    let replay = run(&definition, &initial, 3, 42, true).unwrap();
    let final_only = run(&definition, &initial, 3, 42, false).unwrap();

    assert!(history[1].world.agents.len() > 32);
    assert!(history[1].world.agents.len() < 64);
    assert_eq!(
        serde_json::to_value(&history).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(final_only.len(), 1);
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&history[3]).unwrap()
    );
    for snapshot in &history {
        definition.validate_world(&snapshot.world).unwrap();
    }
}

#[test]
fn reports_id_exhaustion_only_for_accepted_births_without_changing_the_input() {
    let mut definition = definition(Boundary::Wrap);
    definition.rules[0].actions = vec![spawn_action(0.0, 1.0)];
    let initial = world(1, 2, vec![agent(u64::MAX, (0, 0), (0.0, 0.0))]);

    let error = run(&definition, &initial, 1, 42, true).unwrap_err();

    assert!(
        error.contains("Generation 1: Spawn at (0, 1): Agent ID range exhausted"),
        "{error}"
    );
    assert_eq!(initial.agents.len(), 1);
    assert_eq!(initial.agents[0].id, u64::MAX);
    let blocked = world(1, 1, initial.agents.clone());
    assert_eq!(advance(&definition, &blocked).agents.len(), 1);
    let last_available = world(1, 2, vec![agent(u64::MAX - 1, (0, 0), (0.0, 0.0))]);
    assert_eq!(advance(&definition, &last_available).agents[1].id, u64::MAX);
}
