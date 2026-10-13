use std::fmt;

/// Percent-encodes a value using the RFC 3986 unreserved set.
pub fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Builds a query string from key/value pairs, percent-encoding both sides.
pub fn query(pairs: &[(&str, &str)]) -> String {
    let mut builder = QueryString::new();
    for (key, value) in pairs {
        builder.push(key, value);
    }
    builder.build()
}

/// An incremental query-string builder that percent-encodes keys and values.
#[derive(Debug, Default, Clone)]
pub struct QueryString {
    pairs: Vec<(String, String)>,
}

impl QueryString {
    /// Creates an empty query string.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a key/value pair, returning the builder for chaining.
    pub fn push(&mut self, key: &str, value: &str) -> &mut Self {
        self.pairs.push((urlencode(key), urlencode(value)));
        self
    }

    /// Renders the pairs as `k=v&k=v` without a leading `?`.
    pub fn build(&self) -> String {
        self.pairs
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }
}

impl fmt::Display for QueryString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.build())
    }
}
