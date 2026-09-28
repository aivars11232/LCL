//! Writing the updater's own JSON: its state file and its answers to LCL
//! Workspace. Reading JSON uses the engine's std-only parser (`lcl_spec`).

/// `text` as one JSON string, quotes included.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One JSON object, built member by member in the order they are added.
#[derive(Default)]
pub struct Object {
    members: Vec<(String, String)>,
}

impl Object {
    pub fn new() -> Object {
        Object::default()
    }

    /// A member whose value is already JSON.
    pub fn raw(mut self, key: &str, json: String) -> Object {
        self.members.push((key.to_string(), json));
        self
    }

    pub fn str(self, key: &str, value: &str) -> Object {
        self.raw(key, string(value))
    }

    pub fn opt_str(self, key: &str, value: Option<&str>) -> Object {
        match value {
            Some(value) => self.str(key, value),
            None => self.raw(key, "null".to_string()),
        }
    }

    pub fn num(self, key: &str, value: u64) -> Object {
        self.raw(key, value.to_string())
    }

    pub fn bool(self, key: &str, value: bool) -> Object {
        self.raw(key, value.to_string())
    }

    pub fn finish(self) -> String {
        let body: Vec<String> = self
            .members
            .iter()
            .map(|(key, value)| format!("{}: {value}", string(key)))
            .collect();
        format!("{{{}}}", body.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back_exactly() {
        let text = "quote \" back \\ new\nline\ttab \u{1} é";
        let json = Object::new()
            .str("text", text)
            .num("n", 7)
            .bool("b", true)
            .opt_str("none", None)
            .finish();
        let parsed = lcl_spec::json::parse(&json).unwrap();
        assert_eq!(parsed.get("text").and_then(|t| t.as_str()), Some(text));
        assert_eq!(parsed.get("n").and_then(|t| t.as_u64()), Some(7));
        assert_eq!(parsed.get("b").and_then(|t| t.as_bool()), Some(true));
    }
}
