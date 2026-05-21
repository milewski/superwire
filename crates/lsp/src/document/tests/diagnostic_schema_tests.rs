use super::*;

#[test]
fn reports_workflow_compilation_diagnostic_for_non_exhaustive_variant_match() {
    let diagnostics = inline_diagnostics! {
        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        schema event_result {
            event: variant type {
                created {
                    id: string
                }

                deleted {
                    id: string
                }
            }
        }

        agent worker {
            model: model.openai_model
            output {
                value: schema.event_result
            }
        }

        output {
            event_id: match agent.worker.value.event {
                created.id
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::WorkflowCompilationError);
}

#[test]
fn reports_invalid_type_expression_reference_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent greeting {
            output {
                value: test
            }
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidTypeExpressionReference);
}

#[test]
fn accepts_nested_schema_enum_field_reference_diagnostics() {
    let diagnostics = inline_diagnostics! {
        schema main {
            language: enum { en_US, zh_CN, fr }
        }

        input {
            workspace_id: string
            scope: string
        }

        tool create_project_for_workspace {
            description: "Create a new project in the bound workspace and scope."
            input {
                name: [{
                    language: schema.main.language
                    value: string
                }]
                primary_language: schema.main.language
                languages: [schema.main.language]
            }
            bindings {
                workspace_id: input.workspace_id
                scope: input.scope
            }
        }
    };

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidTypeExpressionReference),
        "unexpected invalid type reference diagnostics: {diagnostics:?}"
    );
}
