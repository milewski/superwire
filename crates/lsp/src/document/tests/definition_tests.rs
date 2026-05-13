use super::{source_with_cursor, DocumentState};

#[test]
fn definition_resolves_agent_output_field_in_output_reference() {
    let source_template = inline_document_template! {
        agent task_summary_aggregator {
            model: model.openai_model
            instruction: "aggregate summaries"
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

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to output field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);
}

#[test]
fn definition_resolves_for_loop_binding_field_to_iterable_item_field_declaration() {
    let source_template = inline_document_template! {
        agent participants_fetcher {
            model: model.openai_model
            instruction: "fetch participants"
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
            model: model.openai_model
            instruction: "participant id is {{ participant.<cursor>id }}"
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_field_line = source
        .lines()
        .position(|source_line| source_line.contains("id: number"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include participant id field declaration");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to iterable item field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);
}

#[test]
fn definition_resolves_dynamic_field_reference_and_nested_tool_output_field() {
    let source_template = inline_document_template! {
        tool searchable_web {
            output {
                title: string
                snippet: string
            }
        }

        dynamic {
            search_result: call tool.searchable_web {
                input {
                    query: "release notes"
                }
            }
        }

        output {
            result: dynamic.<cursor>search_result.title
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_field_line = source
        .lines()
        .position(|source_line| source_line.contains("search_result: call tool.searchable_web"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include dynamic search_result field declaration");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to dynamic field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);

    let nested_source_template = inline_document_template! {
        tool searchable_web {
            output {
                title: string
                snippet: string
            }
        }

        dynamic {
            search_result: call tool.searchable_web {
                input {
                    query: "release notes"
                }
            }
        }

        output {
            result: dynamic.search_result.<cursor>title
        }
    };

    let (nested_source, nested_cursor_position) = source_with_cursor(nested_source_template);
    let expected_nested_field_line = nested_source
        .lines()
        .position(|source_line| source_line.contains("title: string"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include tool output title field declaration");

    let nested_document_state = DocumentState::new(nested_source, None);
    let nested_definition_range = nested_document_state
        .definition_range(nested_cursor_position)
        .expect("definition should resolve to tool output field declaration");

    assert_eq!(nested_definition_range.start.line, expected_nested_field_line);
}

#[test]
fn definition_resolves_nested_dynamic_object_literal_field_reference() {
    let source_template = inline_document_template! {
        dynamic {
            metadata: {
                workflow: "dynamic_values"
                version: 1
            }
        }

        output {
            workflow_name: dynamic.metadata.<cursor>workflow
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_field_line = source
        .lines()
        .position(|source_line| source_line.contains("workflow: \"dynamic_values\""))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include nested dynamic workflow field declaration");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to nested dynamic object field declaration");

    assert_eq!(definition_range.start.line, expected_field_line);
}

#[test]
fn definition_resolves_tool_reference_inside_dynamic_tool_call() {
    let source_template = inline_document_template! {
        tool format_response {
            input {
                content: string
            }

            output {
                markdown: string
            }
        }

        dynamic {
            formatted_result: call tool.<cursor>format_response {
                input {
                    content: "hello"
                }
            }
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_tool_line = source
        .lines()
        .position(|source_line| source_line.contains("tool format_response {"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include tool declaration");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to tool declaration");

    assert_eq!(definition_range.start.line, expected_tool_line);
}

#[test]
fn definition_resolves_prompt_reference_inside_multiline_instruction_mcp_call() {
    let source_template = inline_document_template! {
        prompt dynamic_summary_prompt from mcp.local.prompt.dynamic_summary_prompt

        agent writer {
            instruction: """
                Prompt: {{ render prompt.<cursor>dynamic_summary_prompt }}
            """
            output: string
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_prompt_line = source
        .lines()
        .position(|source_line| source_line.contains("prompt dynamic_summary_prompt from mcp.local.prompt.dynamic_summary_prompt"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include prompt import");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to prompt import");

    assert_eq!(definition_range.start.line, expected_prompt_line);
}

#[test]
fn definition_resolves_resource_reference_inside_multiline_instruction_mcp_call() {
    let source_template = inline_document_template! {
        resource project_readme from mcp.local.resource.project_readme

        agent writer {
            instruction: """
                Resource: {{ read resource.<cursor>project_readme }}
            """
            output: string
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_resource_line = source
        .lines()
        .position(|source_line| source_line.contains("resource project_readme from mcp.local.resource.project_readme"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include resource import");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to resource import");

    assert_eq!(definition_range.start.line, expected_resource_line);
}

#[test]
fn definition_resolves_tool_reference_inside_multiline_instruction_tool_call() {
    let source_template = inline_document_template! {
        tool format_response {
            input {
                content: string
            }
        }

        agent writer {
            instruction: """
                Tool: {{ call tool.<cursor>format_response }}
            """
            output: string
        }
    };

    let (source, cursor_position) = source_with_cursor(source_template);
    let expected_tool_line = source
        .lines()
        .position(|source_line| source_line.contains("tool format_response {"))
        .and_then(|line_index| u32::try_from(line_index).ok())
        .expect("source should include tool declaration");

    let document_state = DocumentState::new(source, None);
    let definition_range = document_state
        .definition_range(cursor_position)
        .expect("definition should resolve to tool declaration");

    assert_eq!(definition_range.start.line, expected_tool_line);
}
