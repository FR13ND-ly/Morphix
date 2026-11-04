use std::collections::BTreeMap;

use backend::models::{
    agent::Agent,
    automaton::{AutomatonDefinition, Boundary},
    neighborhood::Neighborhood,
    object_type::{ObjectType, PropertyDefinition},
    position::Position,
    state::ObjectState,
    world::World,
};

fn object_type(id: u8, name: &str, property_name: &str) -> ObjectType {
    ObjectType {
        id,
        name: name.to_owned(),
        properties: BTreeMap::from([(
            property_name.to_owned(),
            PropertyDefinition {
                default_value: 10.0,
                min_value: 0.0,
                max_value: 100.0,
                integer_only: false,
            },
        )]),
    }
}

fn state(type_id: u8, property_name: &str, value: f64) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::from([(property_name.to_owned(), value)]),
    }
}

fn example_automaton() -> AutomatonDefinition {
    AutomatonDefinition {
        name: "Ecosystem".to_owned(),
        object_types: vec![
            object_type(0, "grass", "value"),
            object_type(1, "rabbit", "energy"),
        ],
        neighborhood: Neighborhood {
            offsets: vec![(-1, 0), (1, 0), (0, -1), (0, 1)],
        },
        boundary: Boundary::Wrap,
        rules: Vec::new(),
    }
}

fn example_world() -> World {
    World {
        cells: vec![vec![state(0, "value", 20.0), state(0, "value", 30.0)]],
        agents: vec![Agent {
            id: 7,
            position: Position { row: 0, col: 1 },
            state: state(1, "energy", 12.0),
        }],
    }
}

#[test]
fn accepts_world_when_cells_and_agents_match_the_automaton() {
    let automaton = example_automaton();

    assert!(automaton.validate().is_ok());
    assert!(automaton.validate_world(&example_world()).is_ok());
}

#[test]
fn rejects_empty_definitions_and_duplicate_type_ids_or_names() {
    let mut unnamed = example_automaton();
    unnamed.name = " ".to_owned();
    assert!(unnamed.validate().is_err());

    let mut no_types = example_automaton();
    no_types.object_types.clear();
    assert!(no_types.validate().is_err());

    let mut duplicate_id = example_automaton();
    duplicate_id
        .object_types
        .push(object_type(1, "fox", "energy"));
    assert!(duplicate_id.validate().is_err());

    let mut duplicate_name = example_automaton();
    duplicate_name
        .object_types
        .push(object_type(2, "rabbit", "energy"));
    assert!(duplicate_name.validate().is_err());
}

#[test]
fn rejects_invalid_type_definitions_even_when_not_used_in_the_world() {
    let mut automaton = example_automaton();
    let mut unused_type = object_type(2, "fox", "energy");
    unused_type
        .properties
        .get_mut("energy")
        .unwrap()
        .default_value = 101.0;
    automaton.object_types.push(unused_type);

    assert!(automaton.validate_world(&example_world()).is_err());
}

#[test]
fn rejects_unknown_types_and_identifies_the_cell_or_agent() {
    let automaton = example_automaton();

    let mut unknown_cell = example_world();
    unknown_cell.cells[0][0].type_id = 99;
    let error = automaton.validate_world(&unknown_cell).unwrap_err();
    assert!(error.contains("Cell (0, 0)"));
    assert!(error.contains("Unknown object type id 99"));

    let mut unknown_agent = example_world();
    unknown_agent.agents[0].state.type_id = 99;
    let error = automaton.validate_world(&unknown_agent).unwrap_err();
    assert!(error.contains("Agent 7"));
    assert!(error.contains("Unknown object type id 99"));
}

#[test]
fn validates_properties_of_both_cells_and_agents() {
    let automaton = example_automaton();

    let mut invalid_cell = example_world();
    invalid_cell.cells[0][1]
        .properties
        .insert("value".to_owned(), 101.0);
    let error = automaton.validate_world(&invalid_cell).unwrap_err();
    assert!(error.contains("Cell (0, 1)"));
    assert!(error.contains("value"));

    let mut invalid_agent = example_world();
    invalid_agent.agents[0].state.properties.clear();
    let error = automaton.validate_world(&invalid_agent).unwrap_err();
    assert!(error.contains("Agent 7"));
    assert!(error.contains("Missing property 'energy'"));
}

#[test]
fn also_checks_the_world_structure() {
    let automaton = example_automaton();

    let mut empty_world = example_world();
    empty_world.cells.clear();
    assert!(automaton.validate_world(&empty_world).is_err());

    let mut agent_outside = example_world();
    agent_outside.agents[0].position.col = 2;
    assert!(automaton.validate_world(&agent_outside).is_err());
}

#[test]
fn rejects_invalid_neighborhoods_before_validating_the_world() {
    for offsets in [vec![(0, 0)], vec![(-1, 0), (-1, 0)]] {
        let mut automaton = example_automaton();
        automaton.neighborhood = Neighborhood { offsets };

        assert!(automaton.validate().is_err());
        assert!(automaton.validate_world(&example_world()).is_err());
    }
}

#[test]
fn accepts_fixed_boundary_when_its_state_matches_a_declared_type() {
    let mut automaton = example_automaton();
    automaton.boundary = Boundary::Fixed {
        state: state(0, "value", 0.0),
    };

    assert!(automaton.validate().is_ok());
    assert!(automaton.validate_world(&example_world()).is_ok());
}

#[test]
fn rejects_fixed_boundary_with_an_unknown_type() {
    let mut automaton = example_automaton();
    automaton.boundary = Boundary::Fixed {
        state: state(99, "value", 0.0),
    };

    let error = automaton.validate().unwrap_err();
    assert!(error.contains("Fixed boundary"));
    assert!(error.contains("Unknown object type id 99"));
}

#[test]
fn rejects_invalid_or_missing_properties_in_the_fixed_boundary() {
    let mut automaton = example_automaton();

    for value in [-1.0, 101.0, f64::NAN, f64::INFINITY] {
        automaton.boundary = Boundary::Fixed {
            state: state(0, "value", value),
        };
        assert!(automaton.validate().is_err());
    }

    automaton.boundary = Boundary::Fixed {
        state: ObjectState {
            type_id: 0,
            properties: BTreeMap::new(),
        },
    };
    assert!(automaton.validate_world(&example_world()).is_err());
}
