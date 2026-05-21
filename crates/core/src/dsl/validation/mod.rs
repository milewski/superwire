use super::ast::Workflow;
mod agents;
mod duplicates;
mod dynamic;
mod index;
mod names;
mod references;
mod report;
mod schemas;

use agents::{validate_agent_inference_settings, validate_agent_model_bindings, validate_agent_tool_references};
use dynamic::{validate_agent_dependency_cycles, validate_dynamic_dependency_cycles};
use index::ValidationIndex;
use references::validate_agent_references;
use schemas::validate_schema_references;

pub use report::{SingletonDeclarationKind, ValidationContext, ValidationIssue, ValidationReport};

#[must_use]
pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut validation_report = ValidationReport::default();
    let validation_index = ValidationIndex::build(workflow, &mut validation_report);

    workflow.validate_duplicate_properties(&mut validation_report);
    validate_schema_references(workflow, &validation_index, &mut validation_report);
    validate_agent_inference_settings(workflow, &mut validation_report);
    validate_agent_model_bindings(workflow, &validation_index, &mut validation_report);
    validate_agent_tool_references(workflow, &validation_index, &mut validation_report);
    validate_agent_references(workflow, &validation_index, &mut validation_report);
    validate_dynamic_dependency_cycles(workflow, &mut validation_report);
    validate_agent_dependency_cycles(workflow, &validation_index, &mut validation_report);

    validation_report
}

#[cfg(test)]
mod tests {
    use super::{validate_workflow, SingletonDeclarationKind, ValidationContext, ValidationIssue};
    use crate::dsl::macros::{parse_inline_workflow, workflow_source};
    use crate::dsl::{parse_workflow, ReferenceKeyword};
    use crate::semantic::InferenceSetting;

    macro_rules! assert_issues_contain {
        ($validation_issues:expr, $issue_pattern:pat $(if $guard:expr)? ) => {{
            assert!(
                $validation_issues
                    .iter()
                    .any(|validation_issue| matches!(validation_issue, $issue_pattern $(if $guard)?)),
                "expected matching validation issue; got {:?}",
                $validation_issues
            );
        }};
    }

    macro_rules! assert_workflow_issues_contain {
        ($workflow:expr, $($issue_pattern:pat $(if $guard:expr)?),+ $(,)?) => {{
            let validation_report = validate_workflow(&$workflow);
            let validation_issues = validation_report.issues();

            $(
                assert_issues_contain!(validation_issues, $issue_pattern $(if $guard)?);
            )+
        }};
    }

    macro_rules! assert_workflow_issues_do_not_contain {
        ($workflow:expr, $issue_pattern:pat $(if $guard:expr)? ) => {{
            let validation_report = validate_workflow(&$workflow);
            let validation_issues = validation_report.issues();

            assert!(
                !validation_issues
                    .iter()
                    .any(|validation_issue| matches!(validation_issue, $issue_pattern $(if $guard)?)),
                "did not expect matching validation issue; got {:?}",
                validation_issues
            );
        }};
    }

    #[test]
    fn reports_no_issues_for_valid_workflow() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model openai_model from openai {
                id: "gpt-4.1-mini"
            }

            input {
                title: string
            }

            agent researcher {
                model: model.openai_model
                instruction: input.title
                output {
                    value: string
                }
            }

            output {
                note: agent.researcher.value
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert!(validation_report.is_valid());
        assert!(validation_report.issues().is_empty());
    }

    #[test]
    fn reports_invalid_schema_names() {
        let workflow = parse_inline_workflow! {
            schema ResearchSummary {
                title: string
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidSchemaName { schema_name } if schema_name == "ResearchSummary"
        );
    }

    #[test]
    fn reports_variant_case_discriminator_fields() {
        let workflow = parse_inline_workflow! {
            schema event_payload {
                payload: variant type {
                    user_created {
                        type: string
                        user_id: string
                    }
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidVariantDiscriminatorField { discriminator, case_name }
                if discriminator == "type" && case_name == "user_created"
        );
    }

    #[test]
    fn reports_duplicate_named_resource_names() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}
            provider openai from anthropic {}

            schema user { name: string }
            schema user { id: string }

            tool search { query: string }
            tool search { query: string }

            resource readme from mcp.local.resource.project_readme
            resource readme from mcp.local.resource.project_readme

            prompt system_prompt from mcp.local.prompt.system_prompt
            prompt system_prompt from mcp.local.prompt.system_prompt

            agent researcher {}
            agent researcher {}
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProvider { provider_name } if provider_name == "openai",
            ValidationIssue::DuplicateSchema { schema_name } if schema_name == "user",
            ValidationIssue::DuplicateTool { tool_name } if tool_name == "search",
            ValidationIssue::DuplicateResource { resource_name } if resource_name == "readme",
            ValidationIssue::DuplicatePrompt { prompt_name } if prompt_name == "system_prompt",
            ValidationIssue::DuplicateAgent { agent_name } if agent_name == "researcher"
        );
    }

    #[test]
    fn reports_unknown_tool_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                uses: [tool.web_search]
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownToolReference { tool_name, agent_name }
                if tool_name == "web_search" && agent_name == "researcher"
        );
    }

    #[test]
    fn accepts_declared_tool_reference() {
        let workflow = parse_inline_workflow! {
            tool web_search {
                query: string
            }

            agent researcher {
                uses: [tool.web_search]
            }
        };

        assert_workflow_issues_do_not_contain!(
            workflow,
            ValidationIssue::UnknownToolReference {
                tool_name: _,
                agent_name: _
            }
        );
    }

    #[test]
    fn reports_unknown_mcp_call_references() {
        let workflow = parse_inline_workflow! {
            dynamic {
                readme: read resource.project_readme
                instructions: render prompt.system_prompt
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownResourceReference { resource_name, context }
                if resource_name == "project_readme" && *context == ValidationContext::Dynamic,
            ValidationIssue::UnknownPromptReference { prompt_name, context }
                if prompt_name == "system_prompt" && *context == ValidationContext::Dynamic
        );
    }

    #[test]
    fn accepts_imported_mcp_call_references() {
        let workflow = parse_inline_workflow! {
            resource project_readme from mcp.local.resource.project_readme
            prompt system_prompt from mcp.local.prompt.system_prompt

            dynamic {
                readme: read resource.project_readme
                instructions: render prompt.system_prompt
            }
        };

        assert_workflow_issues_do_not_contain!(
            workflow,
            ValidationIssue::UnknownResourceReference {
                resource_name: _,
                context: _
            }
        );
        assert_workflow_issues_do_not_contain!(
            workflow,
            ValidationIssue::UnknownPromptReference {
                prompt_name: _,
                context: _
            }
        );
    }

    #[test]
    fn reports_wrong_mcp_call_reference_roots() {
        let workflow = parse_inline_workflow! {
            resource project_readme from mcp.local.resource.project_readme
            prompt system_prompt from mcp.local.prompt.system_prompt

            dynamic {
                readme: read prompt.system_prompt
                instructions: render resource.project_readme
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Resource && *context == ValidationContext::Dynamic,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Prompt && *context == ValidationContext::Dynamic
        );
    }

    #[test]
    fn reports_missing_agent_tool_binding_overrides() {
        let workflow_source = workflow_source! {
            input {
                project_id: number
                task_id: number
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
                uses: [tool.fetch_participant_answer]
            }
        };

        assert!(parse_workflow(workflow_source).is_err());
    }

    #[test]
    fn accepts_fixed_tool_bindings_without_agent_overrides() {
        let workflow = parse_inline_workflow! {
            input {
                project_id: number
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

            agent participant_answer_analyzer {
                uses: [tool.fetch_participant_answer]
            }
        };

        assert_workflow_issues_do_not_contain!(
            workflow,
            ValidationIssue::InvalidToolBinding {
                agent_name: _,
                tool_name: _,
                message: _
            }
        );
    }

    #[test]
    fn reports_tool_binding_that_requires_current_agent_output() {
        let workflow = parse_inline_workflow! {
            input {
                workspace_id: number
            }

            tool create_task_group_for_project {
                bindings {
                    workspace_id: input.workspace_id
                    project_id: agent.project_creator.project_id
                }
            }

            agent project_creator {
                uses: [tool.create_task_group_for_project]
                output {
                    project_id: number
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidToolBinding {
                agent_name,
                tool_name,
                message
            } if agent_name == "project_creator"
                && tool_name == "create_task_group_for_project"
                && message.contains("requires its own output")
                && message.contains("Move `tool.create_task_group_for_project` to a later agent")
        );
    }

    #[test]
    fn reports_agent_tool_binding_override_type_mismatch() {
        let workflow_source = workflow_source! {
            input {
                project_id: string
                task_id: number
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

        assert!(parse_workflow(workflow_source).is_err());
    }

    #[test]
    fn reports_agent_binding_override_for_fixed_tool_binding() {
        let workflow = parse_inline_workflow! {
            input {
                project_id: number
                task_id: number
            }

            tool fetch_participant_answer {
                bindings {
                    project_id: input.project_id
                    task_id: input.task_id
                }
            }

            agent participant_answer_analyzer {
                uses: [
                    tool.fetch_participant_answer {
                        bindings {
                            project_id: input.project_id
                        }
                    }
                ]
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidToolBinding {
                agent_name,
                tool_name,
                message
            } if agent_name == "participant_answer_analyzer"
                && tool_name == "fetch_participant_answer"
                && message.contains("already fixed in the tool declaration")
        );
    }

    #[test]
    fn duplicate_schema_diagnostics_include_declaration_span() {
        let workflow_source = "schema user { name: string }\nschema user { id: string }\n";
        let workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let validation_report = validate_workflow(&workflow);

        let duplicate_schema_span = validation_report
            .issues_with_spans()
            .find_map(|(validation_issue, issue_span)| match validation_issue {
                ValidationIssue::DuplicateSchema { schema_name } if schema_name == "user" => issue_span,
                _ => None,
            })
            .expect("duplicate schema diagnostics should include span");

        assert_eq!(duplicate_schema_span.start.line, 2);
        assert_eq!(duplicate_schema_span.start.column, 1);
    }

    #[test]
    fn reports_duplicate_singleton_declarations() {
        let workflow = parse_inline_workflow! {
            input {}
            input {}

            secrets {}
            secrets {}

            output {}
            output {}
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Input,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Secrets,
            ValidationIssue::DuplicateSingletonDeclaration { declaration_kind }
                if *declaration_kind == SingletonDeclarationKind::Output
        );
    }

    #[test]
    fn reports_duplicate_properties_in_declarations_and_object_definitions() {
        let workflow = parse_inline_workflow! {
            provider ollama from ollama {
                endpoint: "http://localhost:11434"
                endpoint: "http://localhost:11435"
            }

            model ollama_model from ollama {
                id: "qwen3.5:8b"
            }

            schema greeting {
                message: string
                message: string
            }

            input {
                profile: {
                    id: string
                    id: string
                }
            }

            agent greeting {
                model: model.ollama_model
                instruction: "hello"
                instruction: "welcome"
                model: model.ollama_model {
                    inference {
                        temperature: 0.2
                        temperature: 0.4
                    }
                }
                output {
                    value: string
                }
            }

            output {
                payload: {
                    status: "ok"
                    status: "ready"
                }
            }
        };

        let validation_report = validate_workflow(&workflow);
        let duplicate_property_issues = validation_report
            .issues()
            .iter()
            .filter(|validation_issue| matches!(validation_issue, ValidationIssue::DuplicateProperty { .. }))
            .count();

        assert!(duplicate_property_issues >= 5);

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "instruction" && *context == ValidationContext::Agent("greeting".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "endpoint" && *context == ValidationContext::Provider("ollama".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "message" && *context == ValidationContext::Schema("greeting".to_string()),
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "id" && *context == ValidationContext::Input,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "status" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_invalid_model_expression() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                model: "gpt-4.1-mini"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidModelExpression { agent_name } if agent_name == "researcher"
        );
    }

    #[test]
    fn reports_invalid_inference_setting_value_type() {
        let workflow = parse_inline_workflow! {
            model fast from openai {
                id: "gpt-4.1-mini"

                inference {
                    temperature: 0.2
                    max_tokens: "2_000"
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name,
                inference_setting
            } if agent_name == "fast" && *inference_setting == InferenceSetting::MaxTokens
        );
    }

    #[test]
    fn reports_direct_agent_inference_as_unknown_property() {
        let workflow = parse_inline_workflow! {
            agent writer {
                inference {
                    max_tokens: 2_000
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownAgentProperty {
                agent_name,
                property_name
            } if agent_name == "writer" && property_name == "inference"
        );
    }

    #[test]
    fn reports_unknown_agent_properties() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model openai_model from openai {
                id: "gpt-4.1-mini"
            }

            agent researcher {
                model: model.openai_model
                instruction: "Analyze this"
                retries: 3
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownAgentProperty {
                agent_name,
                property_name
            } if agent_name == "researcher" && property_name == "retries"
        );
    }

    #[test]
    fn exposes_stable_codes_and_messages_for_validation_issues() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "timeout".to_string(),
        };

        assert_eq!(issue.code(), "unknown_agent_property");
        assert!(issue.message().contains("unsupported property `timeout`"));
    }

    #[test]
    fn unknown_agent_property_diagnostic_suggests_closest_property_name() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "instrction".to_string(),
        };

        let diagnostic = issue.diagnostic(None);
        let help_message = diagnostic.help.expect("unknown property diagnostics should include help");

        assert!(help_message.contains("Did you mean `instruction`?"));
        assert!(help_message.contains("Supported properties:"));
    }

    #[test]
    fn unknown_agent_property_diagnostic_lists_supported_properties_without_guess() {
        let issue = ValidationIssue::UnknownAgentProperty {
            agent_name: "writer".to_string(),
            property_name: "retries".to_string(),
        };

        let diagnostic = issue.diagnostic(None);
        let help_message = diagnostic.help.expect("unknown property diagnostics should include help");

        assert!(help_message.contains("Supported properties:"));
        assert!(!help_message.contains("Did you mean"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn all_validation_issue_diagnostics_include_recovery_help() {
        let validation_issues = vec![
            ValidationIssue::DuplicateProvider {
                provider_name: "openai".to_string(),
            },
            ValidationIssue::DuplicateSchema {
                schema_name: "Result".to_string(),
            },
            ValidationIssue::DuplicateTool {
                tool_name: "search".to_string(),
            },
            ValidationIssue::DuplicateResource {
                resource_name: "readme".to_string(),
            },
            ValidationIssue::DuplicatePrompt {
                prompt_name: "system_prompt".to_string(),
            },
            ValidationIssue::DuplicateAgent {
                agent_name: "writer".to_string(),
            },
            ValidationIssue::DuplicateSingletonDeclaration {
                declaration_kind: SingletonDeclarationKind::Input,
            },
            ValidationIssue::DuplicateProperty {
                property_name: "prompt".to_string(),
                context: ValidationContext::Agent("writer".to_string()),
            },
            ValidationIssue::UnknownAgentProperty {
                agent_name: "writer".to_string(),
                property_name: "prom_t".to_string(),
            },
            ValidationIssue::InvalidInferenceSettingValueType {
                agent_name: "writer".to_string(),
                inference_setting: InferenceSetting::MaxTokens,
            },
            ValidationIssue::InvalidModelExpression {
                agent_name: "writer".to_string(),
            },
            ValidationIssue::UnknownProviderInModel {
                agent_name: "writer".to_string(),
                provider_name: "missing_provider".to_string(),
            },
            ValidationIssue::UnknownModelForProvider {
                agent_name: "writer".to_string(),
                provider_name: "openai".to_string(),
                model_name: "gpt-unknown".to_string(),
            },
            ValidationIssue::UnknownAgentReference {
                referenced_agent: "missing_agent".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownResourceReference {
                resource_name: "readme".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownPromptReference {
                prompt_name: "system_prompt".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidKeywordReferenceRoot {
                keyword: ReferenceKeyword::Input,
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingInputDeclaration {
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingSecretsDeclaration {
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownInputFieldReference {
                field_name: "topic".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::UnknownSecretsFieldReference {
                field_name: "api_key".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::SecretReferenceInLlmContext {
                reference_path: "secrets.api_key".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name: "writer".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::MissingOptionalReferenceAccess {
                reference_path: "agent.writer.payload.value".to_string(),
                field_name: "value".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidReferencePath {
                reference_path: "agent.writer.score".to_string(),
                invalid_field: "score".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::InvalidForLoopIterableType {
                agent_name: "analyzer".to_string(),
                found_type: "{ tasks: [{ id: number }] }".to_string(),
            },
            ValidationIssue::UnknownSchemaReference {
                referenced_schema: "MissingSchema".to_string(),
                context: ValidationContext::Output,
            },
            ValidationIssue::AgentDependencyCycle {
                agent_names: vec!["alpha".to_string(), "beta".to_string()],
            },
        ];

        for validation_issue in validation_issues {
            let diagnostic = validation_issue.diagnostic(None);
            let help_message = diagnostic.help.expect("validation diagnostic should include recovery help");

            assert!(!help_message.trim().is_empty());
        }
    }

    #[test]
    fn reports_secret_reference_leak_in_workflow_output() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            output {
                leaked: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::SecretReferenceInLlmContext { reference_path, context }
                if reference_path == "secrets.api_key" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_secret_reference_leak_in_prompt_interpolation() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            agent researcher {
                instruction: "Use token {{ secrets.api_key }}"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::SecretReferenceInLlmContext { reference_path, context }
                if reference_path == "secrets.api_key"
                    && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn allows_secret_reference_in_provider_configuration() {
        let workflow = parse_inline_workflow! {
            secrets {
                api_key: string
            }

            provider openai from openai {
                api_key: secrets.api_key
            }

            model openai_model from openai {
                id: "gpt-4.1-mini"
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::SecretReferenceInLlmContext { .. });
    }

    #[test]
    fn reports_missing_input_declaration_for_input_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                instruction: input.topic
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingInputDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_unknown_input_field_reference() {
        let workflow = parse_inline_workflow! {
            input {
                title: string
            }

            agent researcher {
                instruction: input.topic
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownInputFieldReference { field_name, context }
                if field_name == "topic" && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reference_diagnostics_include_reference_span() {
        let workflow_source = "input {\n    title: string\n}\n\nagent researcher {\n    instruction: input.topic\n}\n";
        let workflow = parse_workflow(workflow_source).expect("workflow should parse");
        let validation_report = validate_workflow(&workflow);

        let unknown_input_field_span = validation_report
            .issues_with_spans()
            .find_map(|(validation_issue, issue_span)| match validation_issue {
                ValidationIssue::UnknownInputFieldReference { field_name, .. } if field_name == "topic" => issue_span,
                _ => None,
            })
            .expect("unknown input field diagnostics should include span");

        assert_eq!(unknown_input_field_span.start.line, 6);
        assert_eq!(unknown_input_field_span.start.column, 18);
    }

    #[test]
    fn reports_invalid_nested_input_field_reference_path() {
        let workflow = parse_inline_workflow! {
            input {
                profile: {
                    name: string
                }
            }

            agent researcher {
                instruction: input.profile.age
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "input.profile.age"
                && invalid_field == "age"
                && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_missing_secrets_declaration_for_secrets_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                instruction: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingSecretsDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_unknown_secrets_field_reference() {
        let workflow = parse_inline_workflow! {
            secrets {
                openai_key: string
            }

            agent researcher {
                instruction: secrets.api_key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownSecretsFieldReference { field_name, context }
                if field_name == "api_key" && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_invalid_nested_secrets_field_reference_path() {
        let workflow = parse_inline_workflow! {
            secrets {
                credentials: {
                    token: string
                }
            }

            agent researcher {
                instruction: secrets.credentials.key
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "secrets.credentials.key"
                && invalid_field == "key"
                && *context == ValidationContext::Agent("researcher".to_owned())
        );
    }

    #[test]
    fn reports_invalid_bare_keyword_root_references() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                instruction: input
            }

            agent tooling {
                uses: [tool]
            }

            output {
                final: agent
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Input
                    && *context == ValidationContext::Agent("researcher".to_owned()),
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Tool
                    && *context == ValidationContext::Agent("tooling".to_owned()),
            ValidationIssue::InvalidKeywordReferenceRoot { keyword, context }
                if *keyword == ReferenceKeyword::Agent && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_invalid_for_loop_iterable_type_for_object_reference() {
        let workflow = parse_inline_workflow! {
            agent summarizer {
                output {
                    tasks: [{ id: number }]
                    participants: [{ id: number }]
                }
            }

            agent analyzer for participant in agent.summarizer {
                output {
                    value: string
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidForLoopIterableType {
                agent_name,
                found_type: _
            } if agent_name == "analyzer"
        );
    }

    #[test]
    fn allows_for_loop_iterable_type_for_array_reference() {
        let workflow = parse_inline_workflow! {
            agent summarizer {
                output {
                    tasks: [{ id: number }]
                    participants: [{ id: number }]
                }
            }

            agent analyzer for participant in agent.summarizer.participants {
                output {
                    value: string
                }
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidForLoopIterableType { .. });
    }

    #[test]
    fn reports_unknown_provider_for_agent_model() {
        let workflow = parse_inline_workflow! {
            model fast from missing_provider {
                id: "gpt-4.1-mini"
            }

            agent researcher {
                model: model.fast
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownProviderInModelDeclaration {
                model_name,
                provider_name
            } if model_name == "fast" && provider_name == "missing_provider"
        );
    }

    #[test]
    fn reports_unknown_model_for_provider() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            agent researcher {
                model: model.missing_model
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownModelProfile {
                agent_name,
                model_name
            } if agent_name == "researcher" && model_name == "missing_model"
        );
    }

    #[test]
    fn allows_dynamic_model_expression_without_literal_lookup() {
        let workflow = parse_inline_workflow! {
            provider openai from openai {}

            model openai_model from openai {
                id: "gpt-4.1-mini"
            }

            secrets {
                selected_model: string
            }

            agent researcher {
                model: model.openai_model
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidModelExpression { .. });
        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::UnknownModelForProvider { .. });
    }

    #[test]
    fn reports_unknown_agent_references() {
        let workflow = parse_inline_workflow! {
            output {
                note: agent.missing_agent
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownAgentReference {
                referenced_agent,
                context
            } if referenced_agent == "missing_agent" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_missing_agent_output_type_for_nested_agent_field_reference() {
        let workflow = parse_inline_workflow! {
            agent producer {
                instruction: "produce"
            }

            agent consumer {
                instruction: agent.producer.summary
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name,
                context
            } if agent_name == "producer" && *context == ValidationContext::Agent("consumer".to_owned())
        );
    }

    #[test]
    fn reports_missing_agent_output_type_for_output_agent_reference() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                instruction: "Write a short welcome message."
            }

            output {
                greeting: agent.greeting
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingAgentOutputTypeForFieldReference {
                agent_name,
                context
            } if agent_name == "greeting" && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_invalid_nested_agent_output_reference_path() {
        let workflow = parse_inline_workflow! {
            agent producer {
                output {
                    summary: string
                }
            }

            output {
                result: agent.producer.score
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "agent.producer.score"
                && invalid_field == "score"
                && *context == ValidationContext::Output
        );
    }

    #[test]
    fn reports_invalid_field_reference_for_for_loop_agent_final_output_array() {
        let workflow = parse_inline_workflow! {
            agent random for number in [1, 2, 3] {
                instruction: "Give me a random user name and age"
                output {
                    user: (string, number)
                }
            }

            agent surname {
                instruction: "Give a surname to this user {{ agent.random.user }}"
                output {
                    surname: string
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidReferencePath {
                reference_path,
                invalid_field,
                context
            } if reference_path == "agent.random.user"
                && invalid_field == "user"
                && *context == ValidationContext::Agent("surname".to_owned())
        );
    }

    #[test]
    fn reports_missing_optional_reference_access_for_nullable_agent_output_path() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output {
                    nested: maybe {
                        value: string
                    }
                }
            }

            output {
                greeting: agent.greeting.nested.value
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingOptionalReferenceAccess {
                reference_path,
                field_name,
                context
            } if reference_path == "agent.greeting.nested.value"
                && field_name == "value"
                && *context == ValidationContext::Output
        );
    }

    #[test]
    fn accepts_optional_reference_access_for_nullable_agent_output_path() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output {
                    nested: maybe {
                        value: string
                    }
                }
            }

            output {
                greeting: agent.greeting.nested?.value
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::MissingOptionalReferenceAccess { .. });
    }

    #[test]
    fn accepts_valid_nested_agent_output_reference_path() {
        let workflow = parse_inline_workflow! {
            schema report {
                payload: {
                    score: number
                }
            }

            agent producer {
                output {
                    payload: {
                        score: number
                    }
                }
            }

            output {
                result: agent.producer.payload.score
            }
        };

        let validation_report = validate_workflow(&workflow);

        assert!(!validation_report
            .issues()
            .iter()
            .any(|validation_issue| matches!(validation_issue, ValidationIssue::InvalidReferencePath { .. })));
    }

    #[test]
    fn reports_unknown_schema_references() {
        let workflow = parse_inline_workflow! {
            schema wrapper {
                payload: schema.missing_schema
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::UnknownSchemaReference {
                referenced_schema,
                context
            } if referenced_schema == "missing_schema"
                && *context == ValidationContext::Schema("wrapper".to_owned())
        );
    }

    #[test]
    fn allows_schema_field_enum_references_in_type_expressions() {
        let workflow = parse_inline_workflow! {
            schema main {
                language_enum: enum { en_US, zh_CN, fr }
            }

            tool example {
                input {
                    language: schema.main.language_enum
                }
            }
        };

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidTypeExpressionReference { .. });
        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::UnknownSchemaReference { .. });
    }

    #[test]
    fn allows_schema_field_enum_references_inside_nested_array_objects() {
        let workflow = parse_inline_workflow! {
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

        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::InvalidTypeExpressionReference { .. });
        assert_workflow_issues_do_not_contain!(workflow, ValidationIssue::UnknownSchemaReference { .. });
    }

    #[test]
    fn rejects_schema_field_type_references_that_are_not_enums() {
        let workflow = parse_inline_workflow! {
            schema main {
                language: string
            }

            tool example {
                input {
                    language: schema.main.language
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path,
                context
            } if reference_path == "schema.main.language" && *context == ValidationContext::Tool("example".to_owned())
        );
    }

    #[test]
    fn reports_invalid_type_expression_reference_root() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output {
                    value: test
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path,
                context
            } if reference_path == "test" && *context == ValidationContext::Agent("greeting".to_owned())
        );
    }

    #[test]
    fn reports_invalid_keyword_root_in_type_expression_reference() {
        let workflow = parse_inline_workflow! {
            agent greeting {
                output {
                    value: secrets.api_key
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::InvalidTypeExpressionReference {
                reference_path,
                context
            } if reference_path == "secrets.api_key" && *context == ValidationContext::Agent("greeting".to_owned())
        );
    }

    #[test]
    fn reports_agent_dependency_cycles() {
        let workflow = parse_inline_workflow! {
            agent alpha {
                instruction: agent.beta
            }

            agent beta {
                instruction: agent.alpha
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::AgentDependencyCycle { agent_names }
                if agent_names.len() == 2
                    && agent_names.contains(&"alpha".to_owned())
                    && agent_names.contains(&"beta".to_owned())
        );
    }

    #[test]
    fn reports_agent_dependency_cycles_from_interpolated_prompt_bindings() {
        let workflow = parse_inline_workflow! {
            agent alpha {
                instruction: "Something {{ agent.beta }}"
            }

            agent beta {
                instruction: "Something {{ agent.alpha }}"
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::AgentDependencyCycle { agent_names }
                if agent_names.len() == 2
                    && agent_names.contains(&"alpha".to_owned())
                    && agent_names.contains(&"beta".to_owned())
        );
    }

    #[test]
    fn reports_duplicate_dynamic_fields_across_workflow_blocks() {
        let workflow = parse_inline_workflow! {
            dynamic {
                max_results: 5
            }

            dynamic {
                max_results: 10
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::DuplicateProperty { property_name, context }
                if property_name == "max_results" && *context == ValidationContext::Dynamic
        );
    }

    #[test]
    fn reports_missing_dynamic_declaration_for_dynamic_reference() {
        let workflow = parse_inline_workflow! {
            agent researcher {
                instruction: dynamic.topic
                output {
                    value: string
                }
            }
        };

        assert_workflow_issues_contain!(
            workflow,
            ValidationIssue::MissingDynamicDeclaration { context }
                if *context == ValidationContext::Agent("researcher".to_owned())
        );
    }
}
