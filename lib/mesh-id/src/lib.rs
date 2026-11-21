#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// An opaque correlation value shared between operations.
///
/// This wrapper does not parse, validate, or generate an identifier. The inner
/// value is public for compatibility with existing Mesh callers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(
    /// The caller-supplied correlation value, without normalization.
    pub String,
);

impl CorrelationId {
    /// Wraps a value without validating or changing it.
    ///
    /// ```
    /// use mesh_id::CorrelationId;
    /// let id = CorrelationId::new("request-from-another-system");
    /// assert_eq!(id.0, "request-from-another-system");
    /// ```
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }
}

/// Generates a canonical uppercase, 26-character ULID string.
///
/// Generation delegates to [`ulid::Ulid::new`]. This function does not add a
/// monotonic sequence, persistence, or an application-level uniqueness check.
pub fn new_ulid_string() -> String {
    ulid::Ulid::new().to_string()
}

/// Generates a ULID string with lowercase letters.
///
/// The identifier value is unchanged by the case conversion.
pub fn new_lowercase_ulid_string() -> String {
    new_ulid_string().to_ascii_lowercase()
}

/// Combines the supplied prefix, a hyphen, and a new uppercase ULID.
///
/// The prefix is preserved exactly, including case and existing separators.
/// Empty prefixes are accepted. Callers enforce their own prefix policy.
///
/// ```
/// use mesh_id::new_prefixed_id;
/// let id = new_prefixed_id("request");
/// assert!(id.starts_with("request-"));
/// assert_eq!(id.len(), "request-".len() + 26);
/// ```
pub fn new_prefixed_id(prefix: &str) -> String {
    format!("{}-{}", prefix, new_ulid_string())
}

/// Combines the supplied prefix, a hyphen, and a new lowercase ULID.
///
/// Only the generated suffix is lowercased. The prefix is not normalized.
///
/// ```
/// use mesh_id::new_lowercase_prefixed_id;
/// let id = new_lowercase_prefixed_id("API-v2");
/// let suffix = id.strip_prefix("API-v2-").unwrap();
/// assert_eq!(suffix.len(), 26);
/// assert_eq!(suffix, suffix.to_ascii_lowercase());
/// ```
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
