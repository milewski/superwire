use super::*;

#[test]
fn reports_parse_diagnostics_for_invalid_syntax() {
    let diagnostics = inline_diagnostics! {
        agent broken {
            prompt
        } @
    };

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::ParseError);
}

#[test]
fn reports_parse_error_for_call_style_tool_binding_overrides() {
    let diagnostics = inline_diagnostics! {
        agent tooling {
            uses: [
                tool.fetch_participant_answer(project_id: input.project_id, task_id: input.task_id)
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnexpectedRule);
}

#[test]
fn reports_parse_error_for_batch_tool_root_level_binding_fields() {
    let diagnostics = inline_diagnostics! {
        from mcp.local.tool {
            bindings {
                project_id: input.project_id
            }

            tool get_task_group_tasks_tool {
                task_group_id: input.task_group_id
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::ParseError);
}

#[test]
fn reports_parse_error_for_type_syntax_inside_bindings_blocks() {
    let diagnostics = inline_diagnostics! {
        from mcp.mintilify {
            tool query_docs_filesystem_superwire {
                bindings {
                    /// A shell command to run against the virtualized documentation filesystem.
                    command: string
                }
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::ParseError);
}
