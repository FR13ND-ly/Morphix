use crate::models::{
    automaton::{AutomatonDefinition, Boundary},
    cell::{Cell, CellTypeDefinition},
    expression::{Expression as E, Operator as Op},
    grid::Grid,
    rule::{Action, Outcome, Rule},
    simulation::SimulationRequest,
};

fn example(
    name: &str,
    values: Vec<Vec<i64>>,
    neighborhood: Vec<(i32, i32)>,
    expression: E,
    max_value: i64,
) -> SimulationRequest {
    let grid = Grid {
        cells: values
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| Cell {
                        cell_type: 0,
                        value,
                    })
                    .collect()
            })
            .collect(),
    };

    SimulationRequest {
        automaton: AutomatonDefinition {
            name: name.to_owned(),

            cell_types: vec![CellTypeDefinition {
                id: 0,
                name: "cell".to_owned(),
                min_value: 0,
                max_value,
            }],

            neighborhood,

            boundary: Boundary::Fixed {
                cell: Cell {
                    cell_type: 0,
                    value: 0,
                },
            },

            rules: vec![Rule {
                cell_type: 0,
                when: E::Constant(1),

                outcomes: vec![Outcome {
                    probability: 1.0,
                    actions: vec![Action::SetValue { expression }],
                }],
            }],
        },

        grid,
        generations: 1,
        seed: 42,
    }
}

pub fn game_of_life() -> SimulationRequest {
    let live_neighbors = E::CountNeighbors(Box::new(E::binary(
        Op::Equal,
        E::Value,
        E::Constant(1),
    )));

    let born = E::binary(
        Op::Equal,
        live_neighbors.clone(),
        E::Constant(3),
    );

    let survives = E::binary(
        Op::And,
        E::binary(Op::Equal, E::Value, E::Constant(1)),
        E::binary(Op::Equal, live_neighbors, E::Constant(2)),
    );

    let result = E::if_else(
        E::binary(Op::Or, born, survives),
        E::Constant(1),
        E::Constant(0),
    );

    example(
        "Game of Life",
        vec![
            vec![0, 0, 0],
            vec![1, 1, 1],
            vec![0, 0, 0],
        ],
        vec![
            (-1, -1), (-1, 0), (-1, 1),
            (0, -1),           (0, 1),
            (1, -1),  (1, 0),  (1, 1),
        ],
        result,
        1,
    )
}

pub fn sandpile() -> SimulationRequest {
    let unstable = E::binary(
        Op::GreaterOrEqual,
        E::Value,
        E::Constant(4),
    );

    let loss = E::if_else(
        unstable.clone(),
        E::Constant(4),
        E::Constant(0),
    );

    let incoming = E::CountNeighbors(Box::new(unstable));

    let result = E::binary(
        Op::Add,
        E::binary(Op::Subtract, E::Value, loss),
        incoming,
    );

    example(
        "Sandpile",
        vec![
            vec![0, 0, 0],
            vec![0, 4, 0],
            vec![0, 0, 0],
        ],
        vec![(-1, 0), (1, 0), (0, -1), (0, 1)],
        result,
        i64::MAX,
    )
}