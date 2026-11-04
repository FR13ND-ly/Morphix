use std::collections::BTreeMap;

use backend::models::{agent::Agent, position::Position, state::ObjectState, world::World};

fn object_state(type_id: u8) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::new(),
    }
}

fn example_world() -> World {
    World {
        cells: vec![vec![object_state(0); 3]; 2],
        agents: vec![
            Agent {
                id: 7,
                position: Position { row: 0, col: 0 },
                state: object_state(1),
            },
            Agent {
                id: 8,
                position: Position { row: 1, col: 2 },
                state: object_state(1),
            },
        ],
    }
}

#[test]
fn accepts_rectangular_world_with_agents_at_grid_edges() {
    let world = example_world();

    assert_eq!(world.height(), 2);
    assert_eq!(world.width(), 3);
    assert!(world.validate().is_ok());
}

#[test]
fn accepts_worlds_without_agents_and_single_row_or_column_grids() {
    for cells in [
        vec![vec![object_state(0); 5]],
        vec![vec![object_state(0)]; 5],
        vec![vec![object_state(0)]],
    ] {
        let world = World {
            cells,
            agents: Vec::new(),
        };
        assert!(world.validate().is_ok());
    }
}

#[test]
fn rejects_empty_and_non_rectangular_grids() {
    for cells in [
        vec![],
        vec![vec![]],
        vec![vec![object_state(0); 2], vec![object_state(0)]],
        vec![vec![object_state(0); 2], vec![]],
    ] {
        let world = World {
            cells,
            agents: Vec::new(),
        };
        assert!(world.validate().is_err());
    }
}

#[test]
fn rejects_agents_outside_grid_including_very_large_coordinates() {
    for position in [
        Position { row: 2, col: 0 },
        Position { row: 0, col: 3 },
        Position {
            row: usize::MAX,
            col: usize::MAX,
        },
    ] {
        let mut world = example_world();
        world.agents[0].position = position;
        assert!(world.validate().is_err());
    }
}

#[test]
fn rejects_duplicate_agent_ids_even_at_different_positions() {
    let mut world = example_world();
    world.agents[1].id = world.agents[0].id;

    assert!(world.validate().is_err());
}
