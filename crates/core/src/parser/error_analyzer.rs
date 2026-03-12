use crate::parser::Rule;
use pest::error::Error as PestError;

pub struct ErrorAnalyzer {
    #[allow(dead_code)]
    file_path: String,
}

#[derive(Debug)]
pub struct AnalyzedError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub suggestion: Option<String>,
    pub source_line: Option<String>,
}

impl ErrorAnalyzer {
    #[must_use]
    pub const fn new(file_path: String) -> Self {
        Self { file_path }
    }

    #[must_use]
    pub fn analyze(&self, error: &PestError<Rule>, input: &str) -> AnalyzedError {
        let (line, column) = self.extract_position(error);
        let lines: Vec<&str> = input.lines().collect();

        if line == 0 || line > lines.len() {
            return self.create_generic_error(line, column, error, None);
        }

        let context = LineContext::new(&lines, line);

        if let Some(analyzed) = self.analyze_missing_operator(&context) {
            return analyzed;
        }

        if let Some(analyzed) = self.analyze_assignment_operators(&context) {
            return analyzed;
        }

        if let Some(analyzed) = self.analyze_property_names(&context) {
            return analyzed;
        }

        if let Some(analyzed) = self.analyze_missing_properties(&context, &lines) {
            return analyzed;
        }

        if let Some(analyzed) = self.analyze_invalid_references(&context, &lines) {
            return analyzed;
        }

        self.create_generic_error(line, column, error, context.current_line())
    }

    const fn extract_position(&self, error: &PestError<Rule>) -> (usize, usize) {
        match error.line_col {
            pest::error::LineColLocation::Pos((line, column)) => (line, column),
            pest::error::LineColLocation::Span((line, column), _) => (line, column),
        }
    }

    fn analyze_missing_operator(&self, context: &LineContext) -> Option<AnalyzedError> {
        let line = context.current_line()?;
        let line_number = context.line_number;

        if line.contains("<-") || line.contains(':') || line.contains('=') {
            return None;
        }

        let words: Vec<&str> = line.split_whitespace().collect();

        if words.len() >= 2 {
            let first_word = words[0];
            let second_word = words[1];

            let Ok(identifier_pattern) = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$") else {
                return None; // Skip this analysis if regex compilation fails
            };

            if identifier_pattern.is_match(first_word)
                && (second_word.starts_with("agent.")
                    || second_word.starts_with("input.")
                    || second_word.starts_with('"')
                    || identifier_pattern.is_match(second_word))
            {
                let first_word_position = context.find_in_line(first_word)?;
                let position_after_first_word = first_word_position + first_word.len();

                return Some(AnalyzedError {
                    line: line_number,
                    column: position_after_first_word + 1,
                    message: "Missing assignment operator".to_string(),
                    suggestion: Some(format!(
                        "Add '<-' between '{first_word}' and '{second_word}'. Example: {first_word} <- {second_word}"
                    )),
                    source_line: context.current_line_untrimmed().map(std::string::ToString::to_string),
                });
            }
        }

        None
    }

    fn analyze_assignment_operators(&self, context: &LineContext) -> Option<AnalyzedError> {
        let line = context.current_line()?;
        let line_number = context.line_number;

        let operators = [
            (
                ':',
                vec!["==", "!=", "<=", ">=", "<-"],
                "Use '<-' for assignment in output blocks, not ':'. Example: field <- value. Note: ':' is only used for type definitions in schemas",
            ),
            (
                '=',
                vec!["==", "!=", "<=", ">=", "<-"],
                "Use '<-' for assignment, not '='. Example: field <- value",
            ),
            (
                '<',
                vec!["<-", "<="],
                "Use '<-' for assignment, not '<'. Example: field <- value",
            ),
        ];

        for (operator, exclusions, help_message) in operators {
            if self.should_detect_operator(line, operator, &exclusions) {
                if let Some(position) = line.find(operator) {
                    return Some(AnalyzedError {
                        line: line_number,
                        column: position + 1,
                        message: format!("Invalid assignment operator '{operator}'"),
                        suggestion: Some(help_message.to_string()),
                        source_line: Some(line.to_string()),
                    });
                }
            }
        }

        None
    }

    fn should_detect_operator(&self, line: &str, operator: char, exclusions: &[&str]) -> bool {
        if !line.contains(operator) {
            return false;
        }

        for exclusion in exclusions {
            if line.contains(exclusion) {
                return false;
            }
        }

        if operator == ':' {
            let in_braces = line.contains('{') || line.contains('}');
            if in_braces {
                return false;
            }
        }

        true
    }

    fn analyze_property_names(&self, context: &LineContext) -> Option<AnalyzedError> {
        let line = context.current_line()?;
        let line_number = context.line_number;

        if !line.contains("<-") || line.ends_with('{') {
            return None;
        }

        let parts: Vec<&str> = line.split("<-").collect();
        let property_name = parts.first()?.trim();

        if property_name.is_empty() {
            return None;
        }

        let valid_properties = [
            "model",
            "tools",
            "context",
            "output",
            "prompt",
            "for_each",
            "driver",
            "api_endpoint",
            "models",
        ];

        if valid_properties.contains(&property_name) {
            return None;
        }

        let suggestion = self.find_closest_match(property_name, &valid_properties);
        let position = context.find_in_line(property_name)?;

        Some(AnalyzedError {
            line: line_number,
            column: position + 1,
            message: format!("Unknown property '{property_name}'"),
            suggestion: suggestion.map(|s| format!("Did you mean '{s}'?")),
            source_line: Some(line.to_string()),
        })
    }

    fn analyze_missing_properties(&self, context: &LineContext, lines: &[&str]) -> Option<AnalyzedError> {
        let line = context.current_line()?;

        if line != "}" {
            return None;
        }

        let agent_info = self.find_agent_context(lines, context.line_number)?;
        let properties = self.extract_properties(lines, agent_info.start_line, context.line_number);

        let required = [
            ("model", "Add 'model <- \"provider/model\"' to agent"),
            ("prompt", "Add 'prompt <- \"...\"' to agent"),
            ("output", "Add 'output <- {{ ... }}' to agent"),
        ];

        for (prop, suggestion_template) in required {
            if !properties.iter().any(|p| p.starts_with(prop)) {
                return Some(AnalyzedError {
                    line: context.line_number,
                    column: 1,
                    message: format!("Agent '{}' is missing required property '{}'", agent_info.name, prop),
                    suggestion: Some(format!("{} '{}'", suggestion_template, agent_info.name)),
                    source_line: Some(line.to_string()),
                });
            }
        }

        None
    }

    fn analyze_invalid_references(&self, context: &LineContext, lines: &[&str]) -> Option<AnalyzedError> {
        if context.line_number <= 1 {
            return None;
        }

        let previous_line = lines.get(context.line_number - 2)?.trim();

        if !previous_line.contains("<-") || previous_line.ends_with('{') {
            return None;
        }

        let parts: Vec<&str> = previous_line.split("<-").collect();
        if parts.len() != 2 {
            return None;
        }

        let value_part = parts[1].trim();

        if value_part.is_empty()
            || value_part.starts_with('"')
            || value_part.starts_with('[')
            || value_part.starts_with('{')
        {
            return None;
        }

        let Ok(identifier_pattern) = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$") else {
            return None; // Skip this analysis if regex compilation fails
        };
        if !identifier_pattern.is_match(value_part) {
            return None;
        }

        let full_line = lines[context.line_number - 2];
        let arrow_position = full_line.find("<-")?;
        let value_position = full_line[arrow_position..].find(value_part)?;

        Some(AnalyzedError {
            line: context.line_number - 1,
            column: arrow_position + value_position + 1,
            message: format!("Invalid reference: '{value_part}'"),
            suggestion: Some(format!(
                "Did you mean 'agent.{value_part}'? Bare identifiers are not valid values. Use 'agent.name' to reference an agent, or '\"{value_part}\"' for a string literal"
            )),
            source_line: Some(full_line.to_string()),
        })
    }

    fn find_agent_context(&self, lines: &[&str], error_line: usize) -> Option<AgentInfo> {
        let mut brace_count = 0;

        for index in (0..error_line).rev() {
            let line = lines[index].trim();

            if line.ends_with('}') {
                brace_count += 1;
            }

            if line.ends_with('{') {
                if brace_count == 0 {
                    if line.starts_with("agent ") || line.starts_with("<- agent ") {
                        let agent_line = line.trim_start_matches("<-").trim();
                        if let Some(name_part) = agent_line.strip_prefix("agent ") {
                            let agent_name = name_part.trim_end_matches('{').trim().to_string();
                            return Some(AgentInfo {
                                name: agent_name,
                                start_line: index + 1,
                            });
                        }
                    }
                    return None;
                }
                brace_count -= 1;
            }
        }

        None
    }

    fn extract_properties(&self, lines: &[&str], start_line: usize, end_line: usize) -> Vec<String> {
        let mut properties = Vec::new();

        for index in start_line..end_line {
            if index >= lines.len() {
                break;
            }

            let line = lines[index].trim();

            if line.contains("<-") {
                if let Some(property_name) = line.split("<-").next() {
                    properties.push(property_name.trim().to_string());
                }
            }
        }

        properties
    }

    fn find_closest_match(&self, input: &str, candidates: &[&str]) -> Option<String> {
        let input_lower = input.to_lowercase();
        let mut best_match = None;
        let mut best_distance = usize::MAX;

        for candidate in candidates {
            let distance = levenshtein_distance(&input_lower, &candidate.to_lowercase());
            if distance < best_distance && distance <= 2 {
                best_distance = distance;
                best_match = Some(candidate.to_string());
            }
        }

        best_match
    }

    fn create_generic_error(
        &self,
        line: usize,
        column: usize,
        error: &PestError<Rule>,
        source_line: Option<&str>,
    ) -> AnalyzedError {
        let expected_description = self.format_expected_rules(error);

        AnalyzedError {
            line,
            column,
            message: format!("Unexpected syntax: {expected_description}"),
            suggestion: Some("Check the syntax of your workflow definition".to_string()),
            source_line: source_line.map(std::string::ToString::to_string),
        }
    }

    fn format_expected_rules(&self, error: &PestError<Rule>) -> String {
        match &error.variant {
            pest::error::ErrorVariant::ParsingError { positives, .. } => {
                if positives.is_empty() {
                    "unexpected token".to_string()
                } else if positives.len() == 1 {
                    format!("expected {}", rule_to_friendly_name(&positives[0]))
                } else {
                    let names: Vec<String> = positives.iter().map(rule_to_friendly_name).collect();
                    format!("expected one of: {}", names.join(", "))
                }
            }
            _ => "parsing error".to_string(),
        }
    }
}

struct LineContext<'a> {
    lines: &'a [&'a str],
    line_number: usize,
}

impl<'a> LineContext<'a> {
    const fn new(lines: &'a [&'a str], line_number: usize) -> Self {
        Self { lines, line_number }
    }

    fn current_line(&self) -> Option<&'a str> {
        if self.line_number == 0 || self.line_number > self.lines.len() {
            return None;
        }
        Some(self.lines[self.line_number - 1].trim())
    }

    fn current_line_untrimmed(&self) -> Option<&'a str> {
        if self.line_number == 0 || self.line_number > self.lines.len() {
            return None;
        }
        Some(self.lines[self.line_number - 1])
    }

    fn find_in_line(&self, text: &str) -> Option<usize> {
        if self.line_number == 0 || self.line_number > self.lines.len() {
            return None;
        }
        self.lines[self.line_number - 1].find(text)
    }
}

struct AgentInfo {
    name: String,
    start_line: usize,
}

fn levenshtein_distance(source: &str, target: &str) -> usize {
    let source_len = source.len();
    let target_len = target.len();

    if source_len == 0 {
        return target_len;
    }
    if target_len == 0 {
        return source_len;
    }

    let mut matrix = vec![vec![0; target_len + 1]; source_len + 1];

    for (index, row) in matrix.iter_mut().enumerate().take(source_len + 1) {
        row[0] = index;
    }
    #[allow(clippy::needless_range_loop)]
    for index in 0..=target_len {
        matrix[0][index] = index;
    }

    for (source_index, source_char) in source.chars().enumerate() {
        for (target_index, target_char) in target.chars().enumerate() {
            let cost = usize::from(source_char != target_char);
            matrix[source_index + 1][target_index + 1] = std::cmp::min(
                std::cmp::min(
                    matrix[source_index][target_index + 1] + 1,
                    matrix[source_index + 1][target_index] + 1,
                ),
                matrix[source_index][target_index] + cost,
            );
        }
    }

    matrix[source_len][target_len]
}

fn rule_to_friendly_name(rule: &Rule) -> String {
    match rule {
        Rule::string_value => "a string value (e.g., \"text\")".to_string(),
        Rule::agent_property => "an agent property (model, prompt, output, etc.)".to_string(),
        Rule::value => "a value".to_string(),
        Rule::identifier => "an identifier".to_string(),
        Rule::property_prompt => "a prompt property".to_string(),
        Rule::property_model => "a model property".to_string(),
        Rule::property_output => "an output property".to_string(),
        _ => format!("{rule:?}"),
    }
}
