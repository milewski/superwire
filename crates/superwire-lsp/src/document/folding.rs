use super::{DocumentState, FoldingRangeBlock};

impl DocumentState {
    #[must_use]
    pub fn folding_ranges(&self) -> Vec<FoldingRangeBlock> {
        let mut scanner_state = FoldingScannerState::Normal;
        let mut escaped_character_in_string = false;
        let mut opening_brace_stack = Vec::<(u32, u32)>::new();
        let mut folding_ranges = Vec::<FoldingRangeBlock>::new();

        for (line_index, source_line) in self.text.lines().enumerate() {
            let line_number = u32_from_usize_saturating(line_index);
            let line_characters = source_line.chars().collect::<Vec<_>>();
            let mut character_index = 0_usize;

            while character_index < line_characters.len() {
                let current_character = line_characters[character_index];
                let has_triple_quote = has_sequence(&line_characters, character_index, &['"', '"', '"']);

                match scanner_state {
                    FoldingScannerState::TripleQuotedString => {
                        if has_triple_quote {
                            scanner_state = FoldingScannerState::Normal;
                            character_index += 3;

                            continue;
                        }

                        character_index += 1;

                        continue;
                    }
                    FoldingScannerState::QuotedString => {
                        if escaped_character_in_string {
                            escaped_character_in_string = false;
                            character_index += 1;

                            continue;
                        }

                        if current_character == '\\' {
                            escaped_character_in_string = true;
                            character_index += 1;

                            continue;
                        }

                        if current_character == '"' {
                            scanner_state = FoldingScannerState::Normal;
                            character_index += 1;

                            continue;
                        }

                        character_index += 1;

                        continue;
                    }
                    FoldingScannerState::Normal => {}
                }

                if has_sequence(&line_characters, character_index, &['/', '/']) {
                    break;
                }

                if has_triple_quote {
                    scanner_state = FoldingScannerState::TripleQuotedString;
                    character_index += 3;

                    continue;
                }

                if current_character == '"' {
                    scanner_state = FoldingScannerState::QuotedString;
                    escaped_character_in_string = false;
                    character_index += 1;

                    continue;
                }

                if current_character == '{' {
                    opening_brace_stack.push((line_number, u32_from_usize_saturating(character_index)));
                }

                if current_character == '}' {
                    let current_character_position = u32_from_usize_saturating(character_index);

                    if let Some((start_line, start_character)) = opening_brace_stack.pop() {
                        if start_line < line_number {
                            folding_ranges.push(FoldingRangeBlock {
                                start_line,
                                start_character,
                                end_line: line_number,
                                end_character: current_character_position,
                            });
                        }
                    }
                }

                character_index += 1;
            }
        }

        folding_ranges
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoldingScannerState {
    Normal,
    QuotedString,
    TripleQuotedString,
}

fn has_sequence(characters: &[char], start_index: usize, sequence: &[char]) -> bool {
    if start_index + sequence.len() > characters.len() {
        return false;
    }

    characters[start_index..start_index + sequence.len()] == *sequence
}

fn u32_from_usize_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
