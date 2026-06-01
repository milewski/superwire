use serde_json::Value;

pub trait PromptValueFormat {
    fn to_prompt_text(&self) -> String;
}

impl PromptValueFormat for Value {
    fn to_prompt_text(&self) -> String {
        if let Some(string_value) = self.as_str() {
            return string_value.to_string();
        }

        serde_norway::to_string(self).map_or_else(
            |_| serde_json::to_string(self).unwrap_or_else(|_| self.to_string()),
            |serialized_value| serialized_value.trim_end_matches('\n').to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PromptValueFormat;
    use serde_json::json;

    #[test]
    fn renders_top_level_strings_without_yaml_wrapping() {
        assert_eq!(json!("hello world").to_prompt_text(), "hello world");
    }

    #[test]
    fn renders_objects_as_yaml_prompt_text() {
        let value = json!({
            "name": "Alice",
            "age": 30,
            "active": true,
            "tags": ["research", "launch"],
            "profile": {
                "city": "Paris",
                "notes": "first line\nsecond line"
            }
        });

        assert_eq!(
            value.to_prompt_text(),
            "active: true\nage: 30\nname: Alice\nprofile:\n  city: Paris\n  notes: |-\n    first line\n    second line\ntags:\n- research\n- launch"
        );
    }

    #[test]
    fn uses_yaml_quoting_for_ambiguous_nested_strings() {
        let value = json!({
            "empty": "",
            "number_text": "42",
            "plain": "hello world",
            "quoted": "has: separator",
            "truth": "true"
        });

        assert_eq!(
            value.to_prompt_text(),
            "empty: ''\nnumber_text: '42'\nplain: hello world\nquoted: 'has: separator'\ntruth: 'true'"
        );
    }
}
