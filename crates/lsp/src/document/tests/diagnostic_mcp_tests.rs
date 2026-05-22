use super::*;

#[test]
fn reports_mcp_resource_import_names_must_be_snake_case() {
    let diagnostics = inline_diagnostics! {
        mcp local {
            endpoint: "http://localhost:3000"
        }

        resource project_readme from mcp.local.resource.project-readme
    };

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("MCP resource names in .wire files must be snake_case")));
}

#[test]
fn reports_mcp_prompt_import_names_must_be_snake_case() {
    let diagnostics = inline_diagnostics! {
        mcp local {
            endpoint: "http://localhost:3000"
        }

        prompt system_prompt from mcp.local.prompt.system-prompt
    };

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("MCP prompt names in .wire files must be snake_case")));
}

#[test]
fn accepts_manual_mcp_batch_common_output_schema_filter() {
    let source = inline_document_template! {
        from mcp.local.tool {
            output {
                participants: [{  }]
            }

            tool list_all_participants_who_has_answered_given_task
            tool update_user_name
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidToolBinding));
}

#[test]
fn accepts_manual_mcp_tool_output_schema_filter() {
    let source = inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                output {
                    shared: number
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidToolBinding));
}

#[test]
fn reports_invalid_mcp_tool_binding_override_property() {
    let source = inline_document_template! {
        from mcp.local.tool {
            tool list_all_participants_who_has_answered_given_task {
                bindings {
                    unknown: 123
                }
            }
        }
    };
    let document_state = DocumentState::new(source.to_string(), Some(test_mcp_lock()));
    let diagnostics = document_state.diagnostics();

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidToolBinding);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown")));
}
