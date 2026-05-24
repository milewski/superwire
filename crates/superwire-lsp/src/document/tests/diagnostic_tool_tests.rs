use super::*;

#[test]
fn reports_invalid_bare_tool_reference_diagnostic() {
    let diagnostics = inline_diagnostics! {
        agent tooling {
            uses: [tool]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::InvalidKeywordReferenceRoot);
}

#[test]
fn allows_mcp_batch_item_bindings_to_override_shared_bindings() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: number
        }

        from mcp.local {
            bindings {
                project_id: input.project_id
            }

            prompt dynamic_summary_prompt {
                bindings {
                    project_id: 123
                }
            }
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::DuplicateProperty));
}

#[test]
fn allows_block_style_tool_binding_overrides() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: string
            task_id: string
        }

        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        tool fetch_participant_answer {
            bindings {
                project_id: string
                task_id: string
            }
        }

        agent participant_answer_analyzer {
            model: model.openai_model
            uses: [
                tool.fetch_participant_answer {
                    bindings {
                        project_id: input.project_id
                        task_id: input.task_id
                    }
                }
            ]
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::ParseError));
    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::UnknownToolReference));
}

#[test]
fn reports_missing_tool_binding_overrides_diagnostic() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: number
            task_id: number
        }

        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        dynamic {
            data: {
                participants: []
            }
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: number
                task_id: number
            }
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: model.openai_model
            uses: [
                tool.fetch_participant_answer
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnexpectedRule);
}

#[test]
fn allows_fixed_tool_bindings_without_overrides() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: number
        }

        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        dynamic {
            data: {
                participants: []
            }
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: input.project_id
                task_id: 123
            }
        }

        agent participant_answer_analyzer for participant in dynamic.data.participants {
            model: model.openai_model
            uses: [
                tool.fetch_participant_answer
            ]
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::InvalidToolBinding));
}

#[test]
fn reports_invalid_tool_binding_override_type_diagnostic() {
    let diagnostics = inline_diagnostics! {
        input {
            project_id: string
            task_id: number
        }

        provider openai from openai {}

        model openai_model from openai {
            id: "gpt-4.1-mini"
        }

        tool fetch_participant_answer {
            input {
                participant_id: number
            }

            bindings {
                project_id: number
                task_id: number
            }
        }

        agent participant_answer_analyzer {
            model: model.openai_model
            uses: [
                tool.fetch_participant_answer {
                    bindings {
                        project_id: input.project_id
                        task_id: input.task_id
                    }
                }
            ]
        }
    };

    assert_diagnostics_contain_codes!(&diagnostics, DiagnosticCode::UnexpectedRule);
}

#[test]
fn allows_blockless_deterministic_tool_calls() {
    let diagnostics = inline_diagnostics! {
        tool list_participants {
            output {
                count: number
            }
        }

        dynamic {
            data: call tool.list_participants
        }
    };

    assert!(!diagnostic_has_code(&diagnostics, DiagnosticCode::ParseError));
}
