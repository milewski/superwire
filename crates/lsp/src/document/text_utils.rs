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
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?' {
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
    character.is_ascii_alphanumeric() || character == '_' || character == '.' || character == '?'
}
