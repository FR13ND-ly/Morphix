use std::collections::BTreeMap;

use backend::{
    models::{
        action::{Action, NeighborSelection},
        agent::Agent,
        automaton::{AutomatonDefinition, Boundary},
        condition::Condition,
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

fn property(default_value: f64) -> PropertyDefinition {
    PropertyDefinition {
        default_value,
        min_value: 0.0,
        max_value: 100.0,
        integer_only: true,
    }
}

fn ground() -> ObjectState {
    ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("food".to_owned(), 5.0)]),
    }
}

fn agent(id: u64, col: usize, type_id: u8) -> Agent {
    Agent {
        id,
        position: Position { row: 0, col },
        state: ObjectState {
            type_id,
            properties: BTreeMap::from([("energy".to_owned(), 10.0)]),
        },
    }
}

fn world(width: usize, agents: Vec<Agent>) -> World {
    World {
        cells: vec![vec![ground(); width]],
        agents,
    }
}

fn hunt(type_id: u8, target_type: u8) -> Rule {
    Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(type_id),
        probability: Expression::Constant(1.0),
        actions: vec![
            Action::RemoveNeighborAgent {
                type_id: target_type,
                selection: NeighborSelection::First,
            },
            Action::SetProperty {
                property: "energy".to_owned(),
                value: Expression::Add(
                    Box::new(Expression::Property("energy".to_owned())),
                    Box::new(Expression::Constant(10.0)),
                ),
            },
        ],
    }
}

fn definition() -> AutomatonDefinition {
    AutomatonDefinition {
        name: "Agent interactions".to_owned(),
        object_types: vec![
            ObjectType {
                id: 0,
                name: "ground".to_owned(),
                properties: BTreeMap::from([("food".to_owned(), property(5.0))]),
            },
            ObjectType {
                id: 1,
                name: "prey".to_owned(),
                properties: BTreeMap::from([("energy".to_owned(), property(10.0))]),
            },
            ObjectType {
                id: 2,
                name: "predator".to_owned(),
                properties: BTreeMap::from([("energy".to_owned(), property(10.0))]),
            },
        ],
        neighborhood: Neighborhood {
            offsets: vec![(0, -1), (0, 1)],
        },
        boundary: Boundary::Fixed { state: ground() },
        rules: vec![hunt(2, 1)],
    }
}

fn advance(definition: &AutomatonDefinition, initial: &World) -> World {
    let result = step(definition, initial, &mut StdRng::seed_from_u64(42)).unwrap();
    definition.validate_world(&result).unwrap();
    result
}

fn move_by(col: f64) -> Action {
    Action::Move {
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(col),
    }
}

fn spawn(type_id: u8, col: f64) -> Action {
    Action::SpawnAgent {
        type_id,
        row_offset: Expression::Constant(0.0),
        col_offset: Expression::Constant(col),
        properties: BTreeMap::new(),
    }
}

#[test]
fn successful_capture_rewards_the_predator_and_discards_the_victims_other_effects() {
    let mut definition = definition();
    definition.rules[0].actions.extend([
        move_by(1.0),
        Action::SetCellProperty {
            property: "food".to_owned(),
            value: Expression::Constant(0.0),
        },
    ]);
    definition.rules.push(Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(1),
        probability: Expression::Constant(1.0),
        actions: vec![
            move_by(1.0),
            spawn(1, 1.0),
            Action::SetCellProperty {
                property: "food".to_owned(),
                value: Expression::Constant(0.0),
            },
        ],
    });
    definition.rules.push(Rule {
        target: RuleTarget::Cell,
        condition: Condition::Always,
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetProperty {
            property: "food".to_owned(),
            value: Expression::Add(
                Box::new(Expression::Property("food".to_owned())),
                Box::new(Expression::Constant(1.0)),
            ),
        }],
    });
    let initial = world(3, vec![agent(7, 0, 2), agent(9, 1, 1)]);
    let before = serde_json::to_value(&initial).unwrap();
    let next = advance(&definition, &initial);

    assert_eq!(next.agents.len(), 1);
    assert_eq!(next.agents[0].id, 7);
    assert_eq!(next.agents[0].state.properties["energy"], 20.0);
    assert_eq!(next.agents[0].position.col, 1);
    assert_eq!(next.cells[0][0].properties["food"], 0.0);
    assert_eq!(next.cells[0][1].properties["food"], 6.0);
    assert_eq!(next.cells[0][2].properties["food"], 6.0);
    assert_eq!(serde_json::to_value(&initial).unwrap(), before);
}

#[test]
fn contested_captures_cancel_all_attacker_effects_in_either_agent_order() {
    let mut definition = definition();
    definition.rules[0].actions.extend([
        move_by(-1.0),
        spawn(2, -1.0),
        Action::SetCellProperty {
            property: "food".to_owned(),
            value: Expression::Constant(0.0),
        },
    ]);
    for reversed in [false, true] {
        let mut initial = world(5, vec![agent(7, 1, 2), agent(8, 3, 2), agent(9, 2, 1)]);
        if reversed {
            initial.agents.reverse();
        }
        let next = advance(&definition, &initial);
        assert_eq!(
            serde_json::to_value(next).unwrap(),
            serde_json::to_value(initial).unwrap()
        );
    }
}

#[test]
fn no_matching_target_skips_all_effects_and_does_not_try_a_later_rule() {
    let mut definition = definition();
    let zero = Expression::Subtract(
        Box::new(Expression::Property("energy".to_owned())),
        Box::new(Expression::Property("energy".to_owned())),
    );
    definition.rules[0].actions = vec![
        Action::SetType { type_id: 1 },
        Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Divide(Box::new(Expression::Constant(1.0)), Box::new(zero)),
        },
        Action::RemoveNeighborAgent {
            type_id: 1,
            selection: NeighborSelection::First,
        },
    ];
    definition.rules.push(Rule {
        target: RuleTarget::Agent,
        condition: Condition::TypeIs(2),
        probability: Expression::Constant(1.0),
        actions: vec![Action::SetProperty {
            property: "energy".to_owned(),
            value: Expression::Constant(99.0),
        }],
    });
    let initial = world(3, vec![agent(7, 0, 2)]);
    let next = advance(&definition, &initial);
    assert_eq!(
        serde_json::to_value(next).unwrap(),
        serde_json::to_value(initial).unwrap()
    );
}

#[test]
fn first_target_follows_the_observed_agent_list_and_filters_by_type() {
    let definition = definition();
    let initial = world(3, vec![agent(5, 2, 1), agent(7, 1, 2), agent(9, 0, 1)]);
    let next = advance(&definition, &initial);
    assert_eq!(
        next.agents.iter().map(|agent| agent.id).collect::<Vec<_>>(),
        vec![7, 9]
    );
    assert_eq!(next.agents[0].state.properties["energy"], 20.0);
}

#[test]
fn wrapped_offsets_do_not_duplicate_agents_and_only_one_shared_position_occupant_is_removed() {
    let mut definition = definition();
    definition.boundary = Boundary::Wrap;
    definition.neighborhood.offsets = vec![(0, -1), (0, 1), (0, 3)];
    let initial = world(2, vec![agent(7, 0, 2), agent(9, 1, 1), agent(11, 1, 1)]);
    let next = advance(&definition, &initial);
    assert_eq!(
        next.agents.iter().map(|agent| agent.id).collect::<Vec<_>>(),
        vec![7, 11]
    );
    assert_eq!(next.agents[0].state.properties["energy"], 20.0);
    assert_eq!(next.agents[1].position.col, 1);
}

#[test]
fn agents_cannot_target_their_own_position_or_positions_outside_the_neighborhood() {
    let definition = definition();
    for initial in [
        world(3, vec![agent(7, 0, 2)]),
        world(3, vec![agent(7, 0, 2), agent(9, 0, 1)]),
        world(3, vec![agent(7, 0, 2), agent(9, 2, 1)]),
    ] {
        let next = advance(&definition, &initial);
        assert_eq!(
            serde_json::to_value(next).unwrap(),
            serde_json::to_value(initial).unwrap()
        );
    }
    let mut cannibal = definition;
    cannibal.boundary = Boundary::Wrap;
    cannibal.rules = vec![hunt(2, 2)];
    let initial = world(1, vec![agent(7, 0, 2)]);
    assert_eq!(advance(&cannibal, &initial).agents.len(), 1);
}

#[test]
fn a_target_that_removes_itself_does_not_reward_the_attacker() {
    let mut definition = definition();
    definition.rules.insert(
        0,
        Rule {
            target: RuleTarget::Agent,
            condition: Condition::TypeIs(1),
            probability: Expression::Constant(1.0),
            actions: vec![Action::RemoveAgent {}],
        },
    );
    let next = advance(&definition, &world(3, vec![agent(7, 0, 2), agent(9, 1, 1)]));
    assert_eq!(next.agents.len(), 1);
    assert_eq!(next.agents[0].state.properties["energy"], 10.0);
}

#[test]
fn reciprocal_attacks_are_simultaneous_in_either_agent_order() {
    let mut definition = definition();
    definition.rules.push(hunt(1, 2));
    for reversed in [false, true] {
        let mut initial = world(2, vec![agent(7, 0, 2), agent(9, 1, 1)]);
        if reversed {
            initial.agents.reverse();
        }
        assert!(advance(&definition, &initial).agents.is_empty());
    }
}

#[test]
fn accepted_attacks_in_a_food_chain_remain_simultaneous() {
    let mut definition = definition();
    let mut apex_type = definition.object_types[2].clone();
    apex_type.id = 3;
    apex_type.name = "apex".to_owned();
    definition.object_types.push(apex_type);
    definition.rules.push(hunt(3, 2));
    for reversed in [false, true] {
        let mut initial = world(3, vec![agent(11, 0, 3), agent(7, 1, 2), agent(9, 2, 1)]);
        if reversed {
            initial.agents.reverse();
        }
        let next = advance(&definition, &initial);
        assert_eq!(next.agents.len(), 1);
        assert_eq!(next.agents[0].id, 11);
        assert_eq!(next.agents[0].state.properties["energy"], 20.0);
    }
}

#[test]
fn births_can_use_captured_positions_and_do_not_reuse_the_victims_id() {
    let mut definition = definition();
    definition.rules[0].actions.push(spawn(2, 1.0));
    let initial = world(3, vec![agent(7, 0, 2), agent(100, 1, 1)]);
    let history = run(&definition, &initial, 2, 42, true).unwrap();
    assert_eq!(
        history[1]
            .world
            .agents
            .iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>(),
        vec![7, 101]
    );
    assert_eq!(history[1].world.agents[0].state.properties["energy"], 20.0);
    assert_eq!(history[1].world.agents[1].state.properties["energy"], 10.0);
    assert_eq!(history[1].world.agents[1].position.col, 1);
    assert_eq!(
        serde_json::to_value(&history[2].world).unwrap(),
        serde_json::to_value(&history[1].world).unwrap()
    );
}

#[test]
fn later_movement_conflicts_do_not_undo_an_accepted_capture() {
    let mut definition = definition();
    definition.rules[0].actions.push(move_by(2.0));
    let blocker = Agent {
        id: 8,
        position: Position { row: 0, col: 2 },
        state: ground(),
    };
    let next = advance(
        &definition,
        &world(3, vec![agent(7, 0, 2), agent(9, 1, 1), blocker]),
    );
    assert_eq!(next.agents.len(), 2);
    assert_eq!(next.agents[0].id, 7);
    assert_eq!(next.agents[0].position.col, 0);
    assert_eq!(next.agents[0].state.properties["energy"], 20.0);
}

#[test]
fn predator_prey_example_replays_and_starving_predators_eventually_remove_themselves() {
    let request: SimulationRequest =
        serde_json::from_str(include_str!("../examples/predator-prey.json")).unwrap();
    let history = run(
        &request.automaton,
        &request.initial_world,
        40,
        request.seed,
        true,
    )
    .unwrap();
    let replay = run(
        &request.automaton,
        &request.initial_world,
        40,
        request.seed,
        true,
    )
    .unwrap();
    let final_only = run(
        &request.automaton,
        &request.initial_world,
        40,
        request.seed,
        false,
    )
    .unwrap();
    assert_eq!(history[1].world.agents.len(), 2);
    let predator = history[1]
        .world
        .agents
        .iter()
        .find(|agent| agent.state.type_id == 2)
        .unwrap();
    assert_eq!(predator.state.properties["energy"], 20.0);
    assert_eq!(
        serde_json::to_value(&history).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!(final_only.len(), 1);
    assert_eq!(
        serde_json::to_value(&final_only[0]).unwrap(),
        serde_json::to_value(&history[40]).unwrap()
    );
    assert!(
        history[40]
            .world
            .agents
            .iter()
            .all(|agent| agent.state.type_id != 2)
    );
}
