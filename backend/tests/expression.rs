use std::collections::BTreeMap;

use backend::models::{
    agent::Agent,
    automaton::Boundary,
    expression::Expression,
    neighborhood::{Neighborhood, Neighbors},
    position::Position,
    state::ObjectState,
    world::World,
};

fn state(type_id: u8, value: f64) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::from([("value".to_owned(), value)]),
    }
}

fn number(value: f64) -> Box<Expression> {
    Box::new(Expression::Constant(value))
}

#[test]
fn reads_constants_and_current_properties_and_reports_missing_properties() {
    let current = state(0, 12.5);

    assert_eq!(
        Expression::Constant(3.0)
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap(),
        3.0
    );
    assert_eq!(
        Expression::Property("value".to_owned())
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap(),
        12.5
    );
    assert!(
        Expression::Property("missing".to_owned())
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
    );
}

#[test]
fn evaluates_all_arithmetic_operations_including_fractional_division() {
    let current = state(0, 0.0);

    for (expression, expected) in [
        (Expression::Add(number(2.0), number(3.0)), 5.0),
        (Expression::Subtract(number(2.0), number(3.0)), -1.0),
        (Expression::Multiply(number(2.0), number(3.0)), 6.0),
        (Expression::Divide(number(7.0), number(2.0)), 3.5),
    ] {
        assert_eq!(
            expression
                .evaluate(&current, &current, &Neighbors::default())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn counts_neighbors_by_type_and_sums_the_requested_property() {
    let current = state(0, 100.0);
    let first = state(1, 2.0);
    let second = state(2, 3.0);
    let third = state(1, 4.0);
    let neighbors = Neighbors {
        cells: vec![&first, &second, &third],
        ..Neighbors::default()
    };

    let count = Expression::CountNeighbors { type_id: 1 };
    let sum = Expression::SumNeighbors {
        property: "value".to_owned(),
    };

    assert_eq!(count.evaluate(&current, &current, &neighbors).unwrap(), 2.0);
    assert_eq!(sum.evaluate(&current, &current, &neighbors).unwrap(), 9.0);
    assert_eq!(
        count
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap(),
        0.0
    );
    assert_eq!(
        sum.evaluate(&current, &current, &Neighbors::default())
            .unwrap(),
        0.0
    );
}

#[test]
fn reports_missing_neighbor_properties_instead_of_assuming_zero() {
    let current = state(0, 0.0);
    let first = state(0, 2.0);
    let second = ObjectState {
        type_id: 0,
        properties: BTreeMap::new(),
    };
    let expression = Expression::SumNeighbors {
        property: "value".to_owned(),
    };

    let error = expression
        .evaluate(
            &current,
            &current,
            &Neighbors {
                cells: vec![&first, &second],
                ..Neighbors::default()
            },
        )
        .unwrap_err();
    assert!(error.contains("Neighbor 1"));
    assert!(error.contains("value"));
}

#[test]
fn reads_a_specific_neighbors_property_and_combines_it_with_arithmetic() {
    let current = state(1, 100.0);
    let cell = state(0, 200.0);
    let left = state(0, 2.5);
    let right = state(0, 7.25);
    let neighbors = Neighbors {
        offsets: vec![(0, 1), (0, -1)],
        cells: vec![&right, &left],
        ..Neighbors::default()
    };
    let expression = Expression::Subtract(
        Box::new(Expression::NeighborProperty {
            offset: (0, 1),
            property: "value".to_owned(),
        }),
        Box::new(Expression::NeighborProperty {
            offset: (0, -1),
            property: "value".to_owned(),
        }),
    );

    assert_eq!(
        expression.evaluate(&current, &cell, &neighbors).unwrap(),
        4.75
    );
    let restored: Expression =
        serde_json::from_value(serde_json::to_value(&expression).unwrap()).unwrap();
    assert_eq!(
        restored.evaluate(&current, &cell, &neighbors).unwrap(),
        4.75
    );
}

#[test]
fn directional_properties_report_missing_offsets_fields_and_non_finite_values() {
    let current = state(0, 0.0);
    let empty = ObjectState {
        type_id: 0,
        properties: BTreeMap::new(),
    };
    let non_finite = state(0, f64::NAN);
    let expression = Expression::NeighborProperty {
        offset: (0, 1),
        property: "value".to_owned(),
    };
    let error = expression
        .evaluate(&current, &current, &Neighbors::default())
        .unwrap_err();
    assert!(error.contains("offset (0, 1)"), "{error}");
    for (neighbor, message) in [
        (&empty, "missing property 'value'"),
        (&non_finite, "finite"),
    ] {
        let neighbors = Neighbors {
            offsets: vec![(0, 1)],
            cells: vec![neighbor],
            ..Neighbors::default()
        };
        let error = expression
            .evaluate(&current, &current, &neighbors)
            .unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn rejects_non_finite_values_and_overflowing_calculations() {
    let current = state(0, 0.0);

    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            Expression::Constant(value)
                .evaluate(&current, &current, &Neighbors::default())
                .is_err()
        );
        assert!(
            Expression::Property("value".to_owned())
                .evaluate(&state(0, value), &state(0, value), &Neighbors::default())
                .is_err()
        );
        assert!(
            Expression::SumNeighbors {
                property: "value".to_owned()
            }
            .evaluate(
                &current,
                &current,
                &Neighbors {
                    cells: vec![&state(0, value)],
                    ..Neighbors::default()
                }
            )
            .is_err()
        );
    }

    let overflow = Expression::Multiply(number(f64::MAX), number(2.0));
    assert!(
        overflow
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
    );

    let nested_overflow = Expression::Multiply(Box::new(overflow), number(0.0));
    assert!(
        nested_overflow
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
    );

    let neighbor = state(0, f64::MAX);
    assert!(
        Expression::SumNeighbors {
            property: "value".to_owned()
        }
        .evaluate(
            &current,
            &current,
            &Neighbors {
                cells: vec![&neighbor, &neighbor],
                ..Neighbors::default()
            }
        )
        .is_err()
    );
}

#[test]
fn rejects_division_by_positive_or_negative_zero() {
    let current = state(0, 0.0);

    for denominator in [0.0, -0.0] {
        let expression = Expression::Divide(number(1.0), number(denominator));
        assert!(
            expression
                .evaluate(&current, &current, &Neighbors::default())
                .is_err()
        );
    }
}

#[test]
fn evaluates_a_diffusion_formula_using_the_actual_neighborhood_reader() {
    let mut world = World {
        cells: vec![vec![state(0, 0.0); 3]; 3],
        agents: Vec::new(),
    };
    world.cells[1][1]
        .properties
        .insert("value".to_owned(), 100.0);

    let neighborhood = Neighborhood {
        offsets: vec![(-1, 0), (1, 0), (0, -1), (0, 1)],
    };
    let boundary = Boundary::Fixed {
        state: state(0, 0.0),
    };
    let expression = Expression::Divide(
        Box::new(Expression::Add(
            Box::new(Expression::Property("value".to_owned())),
            Box::new(Expression::SumNeighbors {
                property: "value".to_owned(),
            }),
        )),
        number(5.0),
    );

    for (position, expected) in [
        (Position { row: 1, col: 1 }, 20.0),
        (Position { row: 1, col: 0 }, 20.0),
        (Position { row: 0, col: 0 }, 0.0),
    ] {
        let neighbors = neighborhood.read(&world, position, &boundary).unwrap();
        let current = &world.cells[position.row][position.col];
        assert_eq!(
            expression.evaluate(current, current, &neighbors).unwrap(),
            expected
        );
    }
}

#[test]
fn evaluates_powers_using_constant_and_current_state_operands() {
    let current = state(0, 9.0);
    for (expression, expected) in [
        (Expression::Power(number(2.0), number(3.0)), 8.0),
        (Expression::Power(number(2.0), number(-1.0)), 0.5),
        (Expression::Power(number(-2.0), number(3.0)), -8.0),
        (Expression::Power(number(0.0), number(0.0)), 1.0),
        (
            Expression::Power(
                Box::new(Expression::Property("value".to_owned())),
                number(0.5),
            ),
            3.0,
        ),
    ] {
        assert_eq!(
            expression
                .evaluate(&current, &current, &Neighbors::default())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn rejects_power_results_that_are_not_finite_real_numbers() {
    for (base, exponent) in [(-1.0, 0.5), (0.0, -1.0), (1e308, 2.0)] {
        let expression = Expression::Power(
            Box::new(Expression::Property("value".to_owned())),
            number(exponent),
        );
        let error = expression
            .evaluate(&state(0, base), &state(0, base), &Neighbors::default())
            .unwrap_err();
        assert!(error.contains("finite"), "{error}");
    }
}

#[test]
fn calculates_the_probability_of_at_least_one_independent_neighbor_exposure() {
    let expression = Expression::Subtract(
        number(1.0),
        Box::new(Expression::Power(
            Box::new(Expression::Subtract(number(1.0), number(0.2))),
            Box::new(Expression::CountNeighbors { type_id: 1 }),
        )),
    );
    let current = state(0, 0.0);
    let infected = state(1, 0.0);

    for (count, expected) in [(0, 0.0), (1, 0.2), (2, 0.36), (3, 0.488)] {
        let neighbors = Neighbors {
            cells: vec![&infected; count],
            ..Neighbors::default()
        };
        let actual = expression.evaluate(&current, &current, &neighbors).unwrap();
        assert!((actual - expected).abs() < 1e-12);
    }
}

#[test]
fn distinguishes_object_properties_current_cell_properties_and_neighbors() {
    let current = state(1, 12.0);
    let cell = state(0, 3.0);
    let neighbor = state(0, 99.0);
    let neighbors = Neighbors {
        cells: vec![&neighbor],
        ..Neighbors::default()
    };

    assert_eq!(
        Expression::Property("value".to_owned())
            .evaluate(&current, &cell, &neighbors)
            .unwrap(),
        12.0
    );
    assert_eq!(
        Expression::CellProperty("value".to_owned())
            .evaluate(&current, &cell, &neighbors)
            .unwrap(),
        3.0
    );
    assert_eq!(
        Expression::SumNeighbors {
            property: "value".to_owned()
        }
        .evaluate(&current, &cell, &neighbors)
        .unwrap(),
        99.0
    );
}

#[test]
fn passes_the_same_cell_through_nested_arithmetic() {
    let current = state(1, 12.0);
    let cell = state(0, 3.0);
    let expression = Expression::Power(
        Box::new(Expression::Divide(
            Box::new(Expression::Property("value".to_owned())),
            Box::new(Expression::CellProperty("value".to_owned())),
        )),
        number(2.0),
    );

    assert_eq!(
        expression
            .evaluate(&current, &cell, &Neighbors::default())
            .unwrap(),
        16.0
    );
}

#[test]
fn reports_missing_or_invalid_cell_values_instead_of_reading_the_object() {
    let current = state(1, 12.0);
    let empty = ObjectState {
        type_id: 0,
        properties: BTreeMap::new(),
    };
    let property = Expression::CellProperty("value".to_owned());

    let error = property
        .evaluate(&current, &empty, &Neighbors::default())
        .unwrap_err();
    assert!(
        error.contains("Current cell is missing property 'value'"),
        "{error}"
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = property
            .evaluate(&current, &state(0, value), &Neighbors::default())
            .unwrap_err();
        assert!(error.contains("finite"), "{error}");
    }
    let division = Expression::Divide(
        Box::new(Expression::Property("value".to_owned())),
        Box::new(property),
    );
    let error = division
        .evaluate(&current, &state(0, 0.0), &Neighbors::default())
        .unwrap_err();
    assert!(error.contains("Division by zero"), "{error}");
}

fn neighbor_agent(id: u64, type_id: u8, value: f64) -> Agent {
    Agent {
        id,
        position: Position { row: 0, col: 1 },
        state: state(type_id, value),
    }
}

#[test]
fn counts_and_sums_neighbor_agents_by_type_separately_from_neighbor_cells() {
    let current = state(1, 100.0);
    let cell = state(0, 99.0);
    let first = neighbor_agent(7, 1, 4.0);
    let second = neighbor_agent(8, 1, 6.0);
    let other = Agent {
        id: 9,
        position: Position { row: 0, col: 1 },
        state: ObjectState {
            type_id: 2,
            properties: BTreeMap::new(),
        },
    };
    let neighbors = Neighbors {
        cells: vec![&current],
        agents: vec![&first, &other, &second],
        ..Neighbors::default()
    };
    let count = Expression::CountNeighborAgents { type_id: 1 };
    let sum = Expression::SumNeighborAgents {
        type_id: 1,
        property: "value".to_owned(),
    };

    assert_eq!(
        Expression::CountNeighbors { type_id: 1 }
            .evaluate(&current, &cell, &neighbors)
            .unwrap(),
        1.0
    );
    assert_eq!(count.evaluate(&current, &cell, &neighbors).unwrap(), 2.0);
    assert_eq!(sum.evaluate(&current, &cell, &neighbors).unwrap(), 10.0);
    assert_eq!(
        Expression::CountNeighborAgents { type_id: 2 }
            .evaluate(&current, &cell, &neighbors)
            .unwrap(),
        1.0
    );
    assert_eq!(
        count
            .evaluate(&current, &cell, &Neighbors::default())
            .unwrap(),
        0.0
    );
    assert_eq!(
        sum.evaluate(&current, &cell, &Neighbors::default())
            .unwrap(),
        0.0
    );
}

#[test]
fn uses_agent_observations_through_nested_arithmetic_and_reports_empty_average_division() {
    let current = state(0, 0.0);
    let first = neighbor_agent(7, 1, 4.0);
    let second = neighbor_agent(8, 1, 6.0);
    let neighbors = Neighbors {
        agents: vec![&first, &second],
        ..Neighbors::default()
    };
    let expression = Expression::Power(
        Box::new(Expression::Divide(
            Box::new(Expression::SumNeighborAgents {
                type_id: 1,
                property: "value".to_owned(),
            }),
            Box::new(Expression::CountNeighborAgents { type_id: 1 }),
        )),
        number(2.0),
    );

    assert_eq!(
        expression.evaluate(&current, &current, &neighbors).unwrap(),
        25.0
    );
    let error = expression
        .evaluate(&current, &current, &Neighbors::default())
        .unwrap_err();
    assert!(error.contains("Division by zero"), "{error}");
}

#[test]
fn reports_missing_and_non_finite_neighbor_agent_properties() {
    let current = state(0, 0.0);
    let missing = Agent {
        id: 77,
        position: Position { row: 0, col: 1 },
        state: ObjectState {
            type_id: 1,
            properties: BTreeMap::new(),
        },
    };
    let expression = Expression::SumNeighborAgents {
        type_id: 1,
        property: "value".to_owned(),
    };
    let error = expression
        .evaluate(
            &current,
            &current,
            &Neighbors {
                agents: vec![&missing],
                ..Neighbors::default()
            },
        )
        .unwrap_err();
    assert!(
        error.contains("Neighbor agent 77 is missing property 'value'"),
        "{error}"
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let agent = neighbor_agent(7, 1, value);
        let error = expression
            .evaluate(
                &current,
                &current,
                &Neighbors {
                    agents: vec![&agent],
                    ..Neighbors::default()
                },
            )
            .unwrap_err();
        assert!(error.contains("finite"), "{error}");
    }
    let first = neighbor_agent(7, 1, f64::MAX);
    let second = neighbor_agent(8, 1, f64::MAX);
    let error = expression
        .evaluate(
            &current,
            &current,
            &Neighbors {
                agents: vec![&first, &second],
                ..Neighbors::default()
            },
        )
        .unwrap_err();
    assert!(error.contains("finite"), "{error}");
}
