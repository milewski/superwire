use super::{source_with_cursor, DocumentState};

#[test]
fn definition_resolves_agent_output_field_in_output_reference() {
    let source_template = inline_document_template! {
        agent task_summary_aggregator {
            model: openai("gpt-4o")
            prompt: "aggregate summaries"
            output: {
                summary: string
                themes: [{
                    theme: string
                    times: number
                }]
            }
        }

        output {
            summary: agent.task_summary_aggregator.<cursor>summary
            themes: agent.task_summary_aggregator.themes
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_field_line = source
        .lines()
        .position(|source_line| source_line.contains("summary: string"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include summary output field declaration");

    let document_state = DocumentState::new(source);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to output field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);
}

#[test]
fn definition_resolves_for_loop_binding_field_to_iterable_item_field_declaration() {
    let source_template = inline_document_template! {
        agent participants_fetcher {
            model: openai("gpt-4o")
            prompt: "fetch participants"
            output: {
                participants: [
                    {
                        id: number
                        name: string
                    }
                ]
            }
        }

        agent participant_answer_analyzer for participant in agent.participants_fetcher.participants {
            model: openai("gpt-4o")
            prompt: "participant id is {{ participant.<cursor>id }}"
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_field_line = source
        .lines()
        .position(|source_line| source_line.contains("id: number"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include participant id field declaration");

    let document_state = DocumentState::new(source);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to iterable item field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);
}
