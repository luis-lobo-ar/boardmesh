use mesh_id::{
    CorrelationId, new_lowercase_prefixed_id, new_lowercase_ulid_string, new_prefixed_id,
    new_ulid_string,
};
use std::collections::HashSet;

#[test]
fn generated_strings_interoperate_with_the_upstream_ulid_parser() {
    let uppercase = new_ulid_string();
    let lowercase = new_lowercase_ulid_string();
    for value in [&uppercase, &lowercase] {
        let parsed = ulid::Ulid::from_string(value).expect("valid upstream ULID");
        assert_eq!(parsed.to_string(), value.to_ascii_uppercase());
    }
    assert_eq!(uppercase, uppercase.to_ascii_uppercase());
    assert_eq!(lowercase, lowercase.to_ascii_lowercase());
}

#[test]
fn prefix_policy_stays_with_the_caller() {
    for prefix in ["", "API-v2", "request_", "área"] {
        for generate in [new_prefixed_id, new_lowercase_prefixed_id] {
            let value = generate(prefix);
            let suffix = value.strip_prefix(&format!("{prefix}-")).unwrap();
            assert_eq!(suffix.len(), 26);
            assert!(ulid::Ulid::from_string(suffix).is_ok());
        }
    }
}

#[test]
fn correlation_values_remain_opaque_and_hashable() {
    let original = CorrelationId::new("External/value with spaces");
    let mut values = HashSet::new();
    values.insert(original.clone());
    assert!(values.contains(&CorrelationId::new("External/value with spaces")));
    assert_eq!(original.0, "External/value with spaces");
    assert_eq!(CorrelationId::new("").0, "");
}
