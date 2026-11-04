use std::collections::BTreeMap;

use backend::models::{
    agent::Agent,
    automaton::Boundary,
    condition::{Comparison, Condition},
    expression::Expression,
    neighborhood::{Neighborhood, Neighbors},
    position::Position,
    state::ObjectState,
    world::World,
};

fn state(type_id: u8, energy: f64) -> ObjectState {
    ObjectState {
        type_id,
        properties: BTreeMap::from([("energy".to_owned(), energy)]),
    }
}

#[test]
fn checks_cell_type_separately_from_object_type_through_boolean_groups() {
    let current = state(1, 12.0);
    let cell = state(0, 4.0);
    let condition = Condition::All(vec![
        Condition::TypeIs(1),
        Condition::Any(vec![Condition::CellTypeIs(0), Condition::CellTypeIs(2)]),
        Condition::Not(Box::new(Condition::CellTypeIs(1))),
        compare(
            Expression::Property("energy".to_owned()),
            Comparison::Greater,
            Expression::CellProperty("energy".to_owned()),
        ),
    ]);

    assert!(
        condition
            .evaluate(&current, &cell, &Neighbors::default())
            .unwrap()
    );
    assert!(
        !condition
            .evaluate(&current, &state(1, 4.0), &Neighbors::default())
            .unwrap()
    );
    assert!(
        !condition
            .evaluate(&state(0, 12.0), &cell, &Neighbors::default())
            .unwrap()
    );
}

#[test]
fn checks_the_selected_neighbors_type_separately_from_current_object_and_cell() {
    let current = state(1, 12.0);
    let cell = state(0, 4.0);
    let left = state(2, 30.0);
    let right = state(0, 7.0);
    let neighbors = Neighbors {
        offsets: vec![(0, 1), (0, -1)],
        cells: vec![&right, &left],
        ..Neighbors::default()
    };
    let condition = Condition::All(vec![
        Condition::NeighborTypeIs {
            offset: (0, -1),
            type_id: 2,
        },
        Condition::Not(Box::new(Condition::NeighborTypeIs {
            offset: (0, 1),
            type_id: 2,
        })),
    ]);
    assert!(condition.evaluate(&current, &cell, &neighbors).unwrap());
    assert!(
        condition
            .evaluate(&current, &cell, &Neighbors::default())
            .is_err()
    );
}

#[test]
fn neighbor_type_guards_skip_properties_missing_from_other_neighbor_types() {
    let current = state(1, 12.0);
    let rock = ObjectState {
        type_id: 2,
        properties: BTreeMap::new(),
    };
    let neighbors = Neighbors {
        offsets: vec![(0, 1)],
        cells: vec![&rock],
        ..Neighbors::default()
    };
    let has_energy = compare(
        Expression::NeighborProperty {
            offset: (0, 1),
            property: "energy".to_owned(),
        },
        Comparison::Greater,
        Expression::Constant(0.0),
    );
    let guarded = Condition::All(vec![
        Condition::NeighborTypeIs {
            offset: (0, 1),
            type_id: 0,
        },
        has_energy.clone(),
    ]);
    assert!(!guarded.evaluate(&current, &current, &neighbors).unwrap());
    assert!(has_energy.evaluate(&current, &current, &neighbors).is_err());
}

#[test]
fn cell_type_guards_skip_properties_that_do_not_exist_on_other_terrain() {
    let current = state(1, 12.0);
    let rock = ObjectState {
        type_id: 2,
        properties: BTreeMap::new(),
    };
    let has_energy = compare(
        Expression::CellProperty("energy".to_owned()),
        Comparison::Greater,
        Expression::Constant(0.0),
    );
    let guarded = Condition::All(vec![Condition::CellTypeIs(0), has_energy.clone()]);

    assert!(
        !guarded
            .evaluate(&current, &rock, &Neighbors::default())
            .unwrap()
    );
    assert!(
        has_energy
            .evaluate(&current, &rock, &Neighbors::default())
            .is_err()
    );
}

#[test]
fn combines_agent_neighbor_counts_and_property_sums_in_boolean_conditions() {
    let current = state(1, 12.0);
    let cell = state(0, 0.0);
    let prey = Agent {
        id: 7,
        position: Position { row: 0, col: 1 },
        state: state(2, 30.0),
    };
    let neighbors = Neighbors {
        agents: vec![&prey],
        ..Neighbors::default()
    };
    let condition = Condition::All(vec![
        Condition::TypeIs(1),
        Condition::Any(vec![
            compare(
                Expression::CountNeighborAgents { type_id: 2 },
                Comparison::Greater,
                Expression::Constant(0.0),
            ),
            Condition::Not(Box::new(Condition::Always)),
        ]),
        compare(
            Expression::SumNeighborAgents {
                type_id: 2,
                property: "energy".to_owned(),
            },
            Comparison::Greater,
            Expression::Constant(20.0),
        ),
    ]);

    assert!(condition.evaluate(&current, &cell, &neighbors).unwrap());
    assert!(
        !condition
            .evaluate(&current, &cell, &Neighbors::default())
            .unwrap()
    );
}

fn compare(left: Expression, operator: Comparison, right: Expression) -> Condition {
    Condition::Compare {
        left,
        operator,
        right,
    }
}

#[test]
fn compares_numeric_expressions_with_exact_relational_operators() {
    let current = state(1, 12.0);

    for (operator, left, right, expected) in [
        (Comparison::Equal, 3.0, 3.0, true),
        (Comparison::Equal, 3.0, 4.0, false),
        (Comparison::NotEqual, 3.0, 4.0, true),
        (Comparison::NotEqual, 3.0, 3.0, false),
        (Comparison::Less, 2.0, 3.0, true),
        (Comparison::Less, 3.0, 3.0, false),
        (Comparison::LessOrEqual, 3.0, 3.0, true),
        (Comparison::LessOrEqual, 4.0, 3.0, false),
        (Comparison::Greater, 4.0, 3.0, true),
        (Comparison::Greater, 3.0, 3.0, false),
        (Comparison::GreaterOrEqual, 3.0, 3.0, true),
        (Comparison::GreaterOrEqual, 2.0, 3.0, false),
    ] {
        let condition = compare(
            Expression::Constant(left),
            operator,
            Expression::Constant(right),
        );
        assert_eq!(
            condition
                .evaluate(&current, &current, &Neighbors::default())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn combines_type_checks_and_property_comparisons_with_boolean_operations() {
    let has_energy = compare(
        Expression::Property("energy".to_owned()),
        Comparison::Greater,
        Expression::Constant(0.0),
    );
    let condition = Condition::All(vec![
        Condition::Any(vec![Condition::TypeIs(1), Condition::TypeIs(2)]),
        Condition::Not(Box::new(Condition::TypeIs(3))),
        has_energy,
    ]);

    for (current, expected) in [
        (state(1, 12.0), true),
        (state(2, 5.0), true),
        (state(3, 12.0), false),
        (state(1, 0.0), false),
    ] {
        assert_eq!(
            condition
                .evaluate(&current, &current, &Neighbors::default())
                .unwrap(),
            expected
        );
    }

    let current = state(1, 12.0);
    assert!(
        Condition::All(Vec::new())
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );
    assert!(
        !Condition::Any(Vec::new())
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );
}

#[test]
fn short_circuits_boolean_groups_and_propagates_errors_from_needed_branches() {
    let current = state(1, 12.0);
    let missing_property = compare(
        Expression::Property("missing".to_owned()),
        Comparison::Greater,
        Expression::Constant(0.0),
    );

    assert!(
        !Condition::All(vec![Condition::TypeIs(2), missing_property.clone()])
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );
    assert!(
        Condition::Any(vec![Condition::Always, missing_property.clone()])
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );

    assert!(
        Condition::All(vec![Condition::Always, missing_property.clone()])
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
    );
    assert!(
        Condition::Any(vec![Condition::TypeIs(2), missing_property])
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
    );
}

#[test]
fn supports_explicit_absolute_tolerance_without_changing_exact_equality() {
    let current = state(1, 12.0);
    let left = Expression::Add(
        Box::new(Expression::Constant(0.1)),
        Box::new(Expression::Constant(0.2)),
    );
    let right = Expression::Constant(0.3);

    assert!(
        !compare(left.clone(), Comparison::Equal, right.clone())
            .evaluate(&current, &current, &Neighbors::default())
            .unwrap()
    );
    assert!(
        compare(
            left.clone(),
            Comparison::ApproximatelyEqual { tolerance: 1e-12 },
            right.clone()
        )
        .evaluate(&current, &current, &Neighbors::default())
        .unwrap()
    );
    assert!(
        !compare(
            left.clone(),
            Comparison::ApproximatelyEqual { tolerance: 0.0 },
            right.clone()
        )
        .evaluate(&current, &current, &Neighbors::default())
        .unwrap()
    );

    for tolerance in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(
            compare(
                left.clone(),
                Comparison::ApproximatelyEqual { tolerance },
                right.clone()
            )
            .evaluate(&current, &current, &Neighbors::default())
            .is_err()
        );
    }
}

#[test]
fn propagates_invalid_numeric_expression_errors() {
    let current = state(1, 12.0);

    for expression in [
        Expression::Constant(f64::NAN),
        Expression::Divide(
            Box::new(Expression::Constant(1.0)),
            Box::new(Expression::Constant(0.0)),
        ),
    ] {
        let condition = compare(expression, Comparison::Greater, Expression::Constant(0.0));
        assert!(
            condition
                .evaluate(&current, &current, &Neighbors::default())
                .is_err()
        );
    }
}

#[test]
fn evaluates_a_game_of_life_birth_condition_using_the_neighborhood_reader() {
    let neighborhood = Neighborhood {
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
    };
    let boundary = Boundary::Fixed {
        state: state(0, 0.0),
    };
    let condition = Condition::All(vec![
        Condition::TypeIs(0),
        compare(
            Expression::CountNeighbors { type_id: 1 },
            Comparison::Equal,
            Expression::Constant(3.0),
        ),
    ]);

    for (current_type, live_neighbors, expected) in [(0, 3, true), (0, 2, false), (1, 3, false)] {
        let mut world = World {
            cells: vec![vec![state(0, 0.0); 3]; 3],
            agents: Vec::new(),
        };
        world.cells[1][1].type_id = current_type;
        for col in 0..live_neighbors {
            world.cells[0][col].type_id = 1;
        }

        let neighbors = neighborhood
            .read(&world, Position { row: 1, col: 1 }, &boundary)
            .unwrap();
        assert_eq!(
            condition
                .evaluate(&world.cells[1][1], &world.cells[1][1], &neighbors)
                .unwrap(),
            expected
        );
    }
}
