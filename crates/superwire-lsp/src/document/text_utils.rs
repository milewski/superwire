use superwire_dsl::{DeclarationKeyword, ForClauseKeyword};

pub fn trailing_identifier(line_prefix: &str) -> Option<&str> {
    let mut start_index = line_prefix.len();

    for (character_index, character) in line_prefix.char_indices().rev() {
        if character.is_ascii_alphanumeric() || character == '_' {
            start_index = character_index;
            continue;
        }

        break;
    }

    if start_index == line_prefix.len() {
        return None;
    }

    Some(&line_prefix[start_index..])
}

pub fn trailing_reference_token(line_prefix: &str) -> Option<&str> {
    let mut start_index = line_prefix.len();

    for (character_index, character) in line_prefix.char_indices().rev() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?' || character == '*' {
            start_index = character_index;
            continue;
        }

        break;
    }

    if start_index == line_prefix.len() {
        return None;
    }

    Some(&line_prefix[start_index..])
}

pub fn leading_identifier(source_text: &str) -> Option<&str> {
    let mut identifier_end = 0;

    for character in source_text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identifier_end += character.len_utf8();
            continue;
        }

        break;
    }

    if identifier_end == 0 {
        return None;
    }

    let identifier = &source_text[..identifier_end];

    if !is_identifier(identifier) {
        return None;
    }

    Some(identifier)
}

pub fn is_identifier(identifier: &str) -> bool {
    let mut characters = identifier.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };

    if !first_character.is_ascii_alphabetic() && first_character != '_' {
        return false;
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub fn is_inside_interpolation_expression(line_prefix: &str) -> bool {
    let open_count = line_prefix.match_indices("{{").count();
    let close_count = line_prefix.match_indices("}}").count();

    open_count > close_count
}

pub fn is_inside_multiline_string_literal(source_text: &str, cursor_offset: usize) -> bool {
    let source_prefix = &source_text[..cursor_offset];
    let triple_quote_count = source_prefix.match_indices("\"\"\"").count();

    triple_quote_count % 2 == 1
}

pub fn is_symbol_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?' || character == '*'
}

pub fn split_for_clause_binding(source_text: &str) -> Option<(&str, &str)> {
    let trimmed_source = source_text.trim_start();

    if let Some(after_opening_brace) = trimmed_source.strip_prefix('{') {
        return split_object_destructuring_binding(trimmed_source, after_opening_brace);
    }

    let binding_identifier = leading_identifier(trimmed_source)?;
    let remaining_text = &trimmed_source[binding_identifier.len()..];

    Some((binding_identifier, remaining_text))
}

pub fn for_clause_iterable_prefix(line_prefix: &str) -> Option<String> {
    let trimmed_line_prefix = line_prefix.trim_start();
    let agent_keyword_with_space = format!("{} ", DeclarationKeyword::Agent.as_str());
    let (_, after_agent_keyword) = trimmed_line_prefix.rsplit_once(agent_keyword_with_space.as_str())?;
    let for_keyword_with_spaces = format!(" {} ", ForClauseKeyword::For.as_str());
    let (_, after_for_keyword) = after_agent_keyword.split_once(for_keyword_with_spaces.as_str())?;
    let (_, after_for_binding) = split_for_clause_binding(after_for_keyword)?;
    let after_in_keyword = after_for_binding.trim_start().strip_prefix(ForClauseKeyword::In.as_str())?;

    if !after_in_keyword.starts_with(char::is_whitespace) {
        return None;
    }

    let iterable_prefix = after_in_keyword.trim_start();

    if iterable_prefix.contains('{') || iterable_prefix.contains('}') || iterable_prefix.contains(':') {
        return None;
    }

    Some(iterable_prefix.to_string())
}

fn split_object_destructuring_binding<'source>(
    full_binding_source: &'source str,
    mut remaining_text: &'source str,
) -> Option<(&'source str, &'source str)> {
    let mut consumed_length = 1_usize;

    loop {
        let trimmed_remaining_text = remaining_text.trim_start();
        consumed_length += remaining_text.len().saturating_sub(trimmed_remaining_text.len());
        remaining_text = trimmed_remaining_text;

        if let Some(after_closing_brace) = remaining_text.strip_prefix('}') {
            consumed_length += 1;
            let binding_text = &full_binding_source[..consumed_length];

            return Some((binding_text, after_closing_brace));
        }

        let field_identifier = leading_identifier(remaining_text)?;
        consumed_length += field_identifier.len();
        remaining_text = &remaining_text[field_identifier.len()..];

        let trimmed_remaining_text = remaining_text.trim_start();
        consumed_length += remaining_text.len().saturating_sub(trimmed_remaining_text.len());
        remaining_text = trimmed_remaining_text;

        if let Some(after_comma) = remaining_text.strip_prefix(',') {
            consumed_length += 1;
            remaining_text = after_comma;

            continue;
        }

        if let Some(after_closing_brace) = remaining_text.strip_prefix('}') {
            consumed_length += 1;
            let binding_text = &full_binding_source[..consumed_length];

            return Some((binding_text, after_closing_brace));
        }

        return None;
    }
}

#[cfg(test)]
mod tests {
    use super::{for_clause_iterable_prefix, split_for_clause_binding};

    #[test]
    fn splits_identifier_for_clause_binding() {
        let (binding_text, remaining_text) = split_for_clause_binding("item in input.items").expect("identifier binding should parse");

        assert_eq!(binding_text, "item");
        assert_eq!(remaining_text, " in input.items");
    }

    #[test]
    fn splits_object_destructuring_for_clause_binding() {
        let (binding_text, remaining_text) =
            split_for_clause_binding("{ id, name } in agent.alpha.participants").expect("object binding should parse");

        assert_eq!(binding_text, "{ id, name }");
        assert_eq!(remaining_text, " in agent.alpha.participants");
    }

    #[test]
    fn extracts_iterable_prefix_for_identifier_for_clause() {
        let iterable_prefix =
            for_clause_iterable_prefix("agent analyzer for item in input.participants").expect("iterable prefix should parse");

        assert_eq!(iterable_prefix, "input.participants");
    }

    #[test]
    fn extracts_iterable_prefix_for_object_destructuring_for_clause() {
        let iterable_prefix = for_clause_iterable_prefix("agent analyzer for { id, name } in agent.alpha.participants")
            .expect("iterable prefix should parse");

        assert_eq!(iterable_prefix, "agent.alpha.participants");
    }
}
