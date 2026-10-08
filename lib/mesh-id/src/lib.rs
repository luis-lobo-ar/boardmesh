#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }
}

pub fn new_ulid_string() -> String {
    ulid::Ulid::new().to_string()
}

pub fn new_lowercase_ulid_string() -> String {
    new_ulid_string().to_ascii_lowercase()
}

pub fn new_prefixed_id(prefix: &str) -> String {
    format!("{}-{}", prefix, new_ulid_string())
}

pub fn new_lowercase_prefixed_id(prefix: &str) -> String {
    format!("{}-{}", prefix, new_lowercase_ulid_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_id_wraps_the_supplied_value() {
        let id = CorrelationId::new("corr-1");
        assert_eq!(id.0, "corr-1");
    }

    #[test]
    fn new_ulid_string_has_ulid_shape() {
        let id = new_ulid_string();
        assert_eq!(id.len(), 26);
        assert!(id.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn new_lowercase_ulid_string_lowercases_suffix() {
        let id = new_lowercase_ulid_string();
        assert_eq!(id.len(), 26);
        assert_eq!(id, id.to_ascii_lowercase());
    }

    #[test]
    fn new_prefixed_id_adds_prefix_separator() {
        let id = new_prefixed_id("session");
        assert!(id.starts_with("session-"));
        assert_eq!(id["session-".len()..].len(), 26);
    }

    #[test]
    fn new_lowercase_prefixed_id_lowercases_generated_suffix() {
        let id = new_lowercase_prefixed_id("truth");
        assert!(id.starts_with("truth-"));
        assert_eq!(id, id.to_ascii_lowercase());
    }
}
