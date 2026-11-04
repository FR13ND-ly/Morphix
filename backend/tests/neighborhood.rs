use std::collections::BTreeMap;

use backend::models::{
    agent::Agent,
    automaton::Boundary,
    neighborhood::{Neighborhood, Neighbors},
    position::Position,
    state::ObjectState,
    world::World,
};

fn cell(value: f64) -> ObjectState {
    ObjectState {
        type_id: 0,
        properties: BTreeMap::from([("value".to_owned(), value)]),
    }
}

fn example_world() -> World {
    World {
        cells: vec![
            vec![cell(1.0), cell(2.0), cell(3.0)],
            vec![cell(4.0), cell(5.0), cell(6.0)],
            vec![cell(7.0), cell(8.0), cell(9.0)],
        ],
        agents: Vec::new(),
    }
}

fn cardinal_neighborhood() -> Neighborhood {
    Neighborhood {
        offsets: vec![(-1, 0), (1, 0), (0, -1), (0, 1)],
    }
}

fn values(neighbors: Neighbors<'_>) -> Vec<f64> {
    neighbors
        .cells
        .iter()
        .map(|state| state.properties["value"])
        .collect()
}

#[test]
fn accepts_cardinal_surrounding_horizontal_and_custom_neighborhoods() {
    for offsets in [
        vec![(-1, 0), (1, 0), (0, -1), (0, 1)],
        vec![
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ],
        vec![(0, -1), (0, 1)],
        vec![(-2, 0), (0, 2)],
    ] {
        assert!(Neighborhood { offsets }.validate().is_ok());
    }
}

#[test]
fn allows_empty_neighborhood_for_rules_using_only_the_current_object() {
    let neighborhood = Neighborhood {
        offsets: Vec::new(),
    };

    assert!(neighborhood.validate().is_ok());
}

#[test]
fn rejects_the_current_position_as_its_own_neighbor() {
    let neighborhood = Neighborhood {
        offsets: vec![(-1, 0), (0, 0), (1, 0)],
    };

    assert!(neighborhood.validate().is_err());
}

#[test]
fn rejects_repeated_offsets_to_avoid_counting_a_neighbor_twice() {
    let neighborhood = Neighborhood {
        offsets: vec![(-1, 0), (0, 1), (-1, 0)],
    };

    assert!(neighborhood.validate().is_err());
}

#[test]
fn reads_neighbors_in_the_configured_offset_order() {
    let world = example_world();
    let neighborhood = cardinal_neighborhood();
    let boundary = Boundary::Wrap;
    let neighbors = neighborhood
        .read(&world, Position { row: 1, col: 1 }, &boundary)
        .unwrap();

    assert_eq!(values(neighbors), vec![2.0, 8.0, 4.0, 6.0]);
}

#[test]
fn wraps_neighbors_across_all_four_edges() {
    let world = example_world();
    let neighborhood = cardinal_neighborhood();
    let boundary = Boundary::Wrap;

    for (position, expected) in [
        (Position { row: 0, col: 0 }, vec![7.0, 4.0, 3.0, 2.0]),
        (Position { row: 2, col: 2 }, vec![6.0, 3.0, 8.0, 7.0]),
    ] {
        let neighbors = neighborhood.read(&world, position, &boundary).unwrap();
        assert_eq!(values(neighbors), expected);
    }
}

#[test]
fn uses_fixed_state_only_for_positions_outside_the_grid() {
    let world = example_world();
    let neighborhood = cardinal_neighborhood();
    let boundary = Boundary::Fixed { state: cell(0.0) };

    for (position, expected) in [
        (Position { row: 0, col: 0 }, vec![0.0, 4.0, 0.0, 2.0]),
        (Position { row: 1, col: 1 }, vec![2.0, 8.0, 4.0, 6.0]),
    ] {
        let neighbors = neighborhood.read(&world, position, &boundary).unwrap();
        assert_eq!(values(neighbors), expected);
    }
}

#[test]
fn handles_offsets_spanning_multiple_grid_widths_or_heights() {
    let world = example_world();
    let neighborhood = Neighborhood {
        offsets: vec![(-7, 8), (i32::MIN, i32::MAX)],
    };
    let position = Position { row: 1, col: 1 };

    let wrap_boundary = Boundary::Wrap;
    let wrapped = neighborhood.read(&world, position, &wrap_boundary).unwrap();
    assert_eq!(values(wrapped), vec![1.0, 9.0]);

    let fixed_boundary = Boundary::Fixed { state: cell(0.0) };
    let fixed = neighborhood
        .read(&world, position, &fixed_boundary)
        .unwrap();
    assert_eq!(values(fixed), vec![0.0, 0.0]);
}

#[test]
fn keeps_one_result_per_offset_even_if_wrapping_reaches_the_same_cell() {
    let world = World {
        cells: vec![vec![cell(10.0), cell(20.0)]],
        agents: Vec::new(),
    };
    let neighborhood = cardinal_neighborhood();
    let boundary = Boundary::Wrap;
    let neighbors = neighborhood
        .read(&world, Position { row: 0, col: 0 }, &boundary)
        .unwrap();

    assert_eq!(values(neighbors), vec![10.0, 10.0, 20.0, 20.0]);
}

#[test]
fn returns_no_neighbors_when_the_offset_list_is_empty() {
    let neighborhood = Neighborhood {
        offsets: Vec::new(),
    };
    let world = example_world();
    let boundary = Boundary::Wrap;

    assert!(
        neighborhood
            .read(&world, Position { row: 1, col: 1 }, &boundary)
            .unwrap()
            .cells
            .is_empty()
    );
}

#[test]
fn rejects_invalid_worlds_and_positions_before_reading() {
    let neighborhood = cardinal_neighborhood();
    let boundary = Boundary::Wrap;

    let empty = World {
        cells: Vec::new(),
        agents: Vec::new(),
    };
    assert!(
        neighborhood
            .read(&empty, Position { row: 0, col: 0 }, &boundary)
            .is_err()
    );

    let mut ragged = example_world();
    ragged.cells[1].pop();
    assert!(
        neighborhood
            .read(&ragged, Position { row: 0, col: 0 }, &boundary)
            .is_err()
    );

    let world = example_world();
    for position in [
        Position { row: 3, col: 0 },
        Position { row: 0, col: 3 },
        Position {
            row: usize::MAX,
            col: usize::MAX,
        },
    ] {
        assert!(neighborhood.read(&world, position, &boundary).is_err());
    }
}

#[test]
fn rejects_invalid_neighborhood_definitions_when_reading() {
    let world = example_world();
    let boundary = Boundary::Wrap;

    for offsets in [vec![(0, 0)], vec![(0, 1), (0, 1)]] {
        let neighborhood = Neighborhood { offsets };
        assert!(
            neighborhood
                .read(&world, Position { row: 1, col: 1 }, &boundary)
                .is_err()
        );
    }
}

fn agent_at(id: u64, row: usize, col: usize) -> Agent {
    Agent {
        id,
        position: Position { row, col },
        state: ObjectState {
            type_id: 1,
            properties: BTreeMap::new(),
        },
    }
}

fn agent_ids(neighbors: &Neighbors<'_>) -> Vec<u64> {
    neighbors.agents.iter().map(|agent| agent.id).collect()
}

#[test]
fn finds_all_agents_at_visible_positions_in_world_list_order() {
    let mut world = example_world();
    world.agents = vec![
        agent_at(7, 1, 1),
        agent_at(8, 0, 1),
        agent_at(9, 1, 2),
        agent_at(10, 0, 0),
        agent_at(11, 0, 1),
        agent_at(12, 1, 1),
    ];

    let neighbors = cardinal_neighborhood()
        .read(&world, Position { row: 1, col: 1 }, &Boundary::Wrap)
        .unwrap();

    assert_eq!(agent_ids(&neighbors), vec![8, 9, 11]);
    assert!(std::ptr::eq(neighbors.agents[0], &world.agents[1]));
    assert_eq!(neighbors.cells.len(), 4);
    assert_eq!(world.agents.len(), 6);
}

#[test]
fn wraps_agent_positions_without_duplicates_or_agents_from_the_current_position() {
    let world = World {
        cells: vec![vec![cell(10.0), cell(20.0)]],
        agents: vec![
            agent_at(7, 0, 0),
            agent_at(8, 0, 1),
            agent_at(9, 0, 0),
            agent_at(10, 0, 1),
        ],
    };

    let neighbors = cardinal_neighborhood()
        .read(&world, Position { row: 0, col: 0 }, &Boundary::Wrap)
        .unwrap();

    assert_eq!(agent_ids(&neighbors), vec![8, 10]);
    assert_eq!(values(neighbors), vec![10.0, 10.0, 20.0, 20.0]);
    let one_cell = World {
        cells: vec![vec![cell(0.0)]],
        agents: vec![agent_at(7, 0, 0), agent_at(8, 0, 0)],
    };
    let neighbors = cardinal_neighborhood()
        .read(&one_cell, Position { row: 0, col: 0 }, &Boundary::Wrap)
        .unwrap();
    assert_eq!(neighbors.cells.len(), 4);
    assert!(neighbors.agents.is_empty());
}

#[test]
fn fixed_boundaries_supply_cells_but_do_not_create_neighbor_agents() {
    let world = World {
        cells: vec![vec![cell(0.0), cell(1.0)]],
        agents: vec![agent_at(7, 0, 0), agent_at(8, 0, 1)],
    };
    let boundary = Boundary::Fixed {
        state: ObjectState {
            type_id: 1,
            properties: BTreeMap::new(),
        },
    };

    let neighbors = cardinal_neighborhood()
        .read(&world, Position { row: 0, col: 0 }, &boundary)
        .unwrap();

    assert_eq!(agent_ids(&neighbors), vec![8]);
    assert_eq!(
        neighbors
            .cells
            .iter()
            .filter(|state| state.type_id == 1)
            .count(),
        3
    );
}

#[test]
fn an_empty_offset_list_observes_neither_cells_nor_agents() {
    let mut world = example_world();
    world.agents = vec![agent_at(7, 1, 1), agent_at(8, 1, 2)];

    let neighbors = Neighborhood {
        offsets: Vec::new(),
    }
    .read(&world, Position { row: 1, col: 1 }, &Boundary::Wrap)
    .unwrap();

    assert!(neighbors.cells.is_empty());
    assert!(neighbors.agents.is_empty());
    assert!(neighbors.free_offsets.is_empty());
}

#[test]
fn custom_and_extreme_offsets_select_agents_using_the_same_coordinates_as_cells() {
    let mut world = example_world();
    world.agents = vec![agent_at(7, 0, 0), agent_at(8, 2, 2), agent_at(9, 1, 0)];
    let neighborhood = Neighborhood {
        offsets: vec![(-7, 8), (i32::MIN, i32::MAX)],
    };

    let neighbors = neighborhood
        .read(&world, Position { row: 1, col: 1 }, &Boundary::Wrap)
        .unwrap();

    assert_eq!(agent_ids(&neighbors), vec![7, 8]);
    assert_eq!(values(neighbors), vec![1.0, 9.0]);
}

#[test]
fn selects_cells_by_offset_independently_of_the_configured_order() {
    let world = example_world();
    let neighborhood = Neighborhood {
        offsets: vec![(0, 1), (-1, 0), (0, -1), (1, 0)],
    };
    let neighbors = neighborhood
        .read(&world, Position { row: 1, col: 1 }, &Boundary::Wrap)
        .unwrap();

    assert_eq!(neighbors.offsets, neighborhood.offsets);
    for (offset, expected) in [((0, -1), 4.0), ((0, 1), 6.0), ((-1, 0), 2.0), ((1, 0), 8.0)] {
        neighborhood.validate_offset(offset).unwrap();
        assert_eq!(
            neighbors.cell_at_offset(offset).unwrap().properties["value"],
            expected
        );
    }
    for offset in [(0, 0), (2, 0)] {
        assert!(neighborhood.validate_offset(offset).is_err());
        assert!(neighbors.cell_at_offset(offset).is_err());
    }
    assert!(Neighbors::default().cell_at_offset((0, 1)).is_err());
}

#[test]
fn directional_reads_keep_wrapped_aliases_and_the_fixed_outside_state() {
    let world = World {
        cells: vec![vec![cell(1.0), cell(2.0)]],
        agents: Vec::new(),
    };
    let neighborhood = Neighborhood {
        offsets: vec![(0, 1), (0, -1), (0, 2)],
    };
    let position = Position { row: 0, col: 0 };
    let wrapped = neighborhood
        .read(&world, position, &Boundary::Wrap)
        .unwrap();
    assert_eq!(
        wrapped.cell_at_offset((0, -1)).unwrap().properties["value"],
        2.0
    );
    assert_eq!(
        wrapped.cell_at_offset((0, 1)).unwrap().properties["value"],
        2.0
    );
    assert_eq!(
        wrapped.cell_at_offset((0, 2)).unwrap().properties["value"],
        1.0
    );

    let boundary = Boundary::Fixed { state: cell(99.0) };
    let fixed = neighborhood.read(&world, position, &boundary).unwrap();
    assert_eq!(
        fixed.cell_at_offset((0, -1)).unwrap().properties["value"],
        99.0
    );
    assert_eq!(
        fixed.cell_at_offset((0, 1)).unwrap().properties["value"],
        2.0
    );
    assert_eq!(
        fixed.cell_at_offset((0, 2)).unwrap().properties["value"],
        99.0
    );
}

#[test]
fn free_neighbors_exclude_occupied_positions_and_keep_the_configured_order() {
    let mut world = example_world();
    world.agents = vec![agent_at(7, 1, 1), agent_at(8, 1, 2), agent_at(9, 0, 1)];
    let neighbors = cardinal_neighborhood()
        .read(&world, Position { row: 1, col: 1 }, &Boundary::Wrap)
        .unwrap();

    assert_eq!(neighbors.free_offsets, vec![(1, 0), (0, -1)]);
    assert_eq!(agent_ids(&neighbors), vec![8, 9]);
}

#[test]
fn free_neighbors_deduplicate_wrapped_positions_and_exclude_self_and_fixed_outside() {
    let mut world = World {
        cells: vec![vec![cell(1.0); 3]],
        agents: Vec::new(),
    };
    world.agents = vec![agent_at(7, 0, 0), agent_at(8, 0, 1)];
    let neighborhood = Neighborhood {
        offsets: vec![(0, -1), (0, 2), (0, 3), (0, 1)],
    };
    let position = Position { row: 0, col: 0 };
    let wrapped = neighborhood
        .read(&world, position, &Boundary::Wrap)
        .unwrap();
    assert_eq!(wrapped.free_offsets, vec![(0, -1)]);
    assert_eq!(wrapped.cells.len(), 4);

    let boundary = Boundary::Fixed { state: cell(0.0) };
    let fixed = neighborhood.read(&world, position, &boundary).unwrap();
    assert_eq!(fixed.free_offsets, vec![(0, 2)]);

    let one_cell = World {
        cells: vec![vec![cell(0.0)]],
        agents: vec![agent_at(7, 0, 0)],
    };
    let wrapped = neighborhood
        .read(&one_cell, position, &Boundary::Wrap)
        .unwrap();
    assert!(wrapped.free_offsets.is_empty());
}
