use std::collections::BTreeMap;

use backend::models::{
    object_type::{ObjectType, PropertyDefinition},
    state::ObjectState,
};

fn rabbit_type() -> ObjectType {
    ObjectType {
        id: 2,
        name: "rabbit".to_owned(),
        properties: BTreeMap::from([
            (
                "energy".to_owned(),
                PropertyDefinition {
                    default_value: 10.0,
                    min_value: 0.0,
                    max_value: 100.0,
                    integer_only: false,
                },
            ),
            (
                "age".to_owned(),
                PropertyDefinition {
                    default_value: 0.0,
                    min_value: 0.0,
                    max_value: 100.0,
                    integer_only: true,
                },
            ),
        ]),
    }
}

fn rabbit_state(energy: f64, age: f64) -> ObjectState {
    ObjectState {
        type_id: 2,
        properties: BTreeMap::from([("energy".to_owned(), energy), ("age".to_owned(), age)]),
    }
}

#[test]
fn accepts_valid_definitions_and_states_including_range_boundaries() {
    let object_type = rabbit_type();
    assert!(object_type.validate().is_ok());

    for state in [
        rabbit_state(12.5, 3.0),
        rabbit_state(0.0, 0.0),
        rabbit_state(100.0, 100.0),
    ] {
        assert!(object_type.validate_state(&state).is_ok());
    }

    let mut empty_type = rabbit_type();
    empty_type.properties.clear();
    let empty_state = ObjectState {
        type_id: 2,
        properties: BTreeMap::new(),
    };
    assert!(empty_type.validate_state(&empty_state).is_ok());
}

#[test]
fn rejects_invalid_type_and_property_definitions() {
    let mut unnamed_type = rabbit_type();
    unnamed_type.name = " ".to_owned();
    assert!(unnamed_type.validate().is_err());

    let mut unnamed_property = rabbit_type();
    let property = unnamed_property.properties.remove("age").unwrap();
    unnamed_property.properties.insert(" ".to_owned(), property);
    assert!(unnamed_property.validate().is_err());

    for (min_value, max_value, default_value, integer_only) in [
        (f64::NAN, 100.0, 10.0, false),
        (0.0, f64::INFINITY, 10.0, false),
        (100.0, 0.0, 10.0, false),
        (0.0, 100.0, -1.0, false),
        (0.0, 100.0, f64::NAN, false),
        (0.0, 100.0, 1.5, true),
    ] {
        let mut object_type = rabbit_type();
        object_type.properties.insert(
            "energy".to_owned(),
            PropertyDefinition {
                default_value,
                min_value,
                max_value,
                integer_only,
            },
        );
        assert!(object_type.validate().is_err());
    }
}

#[test]
fn rejects_non_finite_out_of_range_and_fractional_integer_values() {
    let object_type = rabbit_type();

    for energy in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 101.0] {
        assert!(
            object_type
                .validate_state(&rabbit_state(energy, 3.0))
                .is_err()
        );
    }

    assert!(
        object_type
            .validate_state(&rabbit_state(12.0, 3.5))
            .is_err()
    );
}

#[test]
fn requires_matching_type_and_all_declared_properties_without_extras() {
    let object_type = rabbit_type();

    let mut wrong_type = rabbit_state(12.0, 3.0);
    wrong_type.type_id = 3;
    assert!(object_type.validate_state(&wrong_type).is_err());

    let mut missing_property = rabbit_state(12.0, 3.0);
    missing_property.properties.remove("age");
    assert!(object_type.validate_state(&missing_property).is_err());

    let mut extra_property = rabbit_state(12.0, 3.0);
    extra_property.properties.insert("speed".to_owned(), 1.0);
    assert!(object_type.validate_state(&extra_property).is_err());
}
