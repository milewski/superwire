use std::collections::BTreeMap;

use superwire_core::dsl::{validate_workflow, Reference, ReferenceAccess, ReferenceKeyword, ReferenceRoot, TypeExpression};
use superwire_core::semantic::support::types::WorkflowType;
use superwire_core::semantic::{
    ProviderDriver, ReferenceResolutionScope, SemanticDeclarationKey, SemanticFieldRoot, SemanticMcpImportKind, WorkflowSemanticIndex,
};

macro_rules! parse_inline_workflow {
    ($($workflow_tokens:tt)*) => {{
        let workflow_source_template = superwire_core::testing::WorkflowSourceTemplate::from_inline(stringify!($($workflow_tokens)*));
        workflow_source_template.parse_workflow().unwrap_or_else(|parse_error| {
            panic!(
                "inline workflow failed to parse:\n{}",
                parse_error.render_with_source(workflow_source_template.source(), "<inline workflow>")
            )
        })
    }};
}

#[test]
fn workflow_semantic_index_exposes_declaration_lookup() {
    let workflow = parse_inline_workflow! {
        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1-mini"
        }

        schema report {
            title: string
        }

        input {
            topic: string
        }

        secrets {
            api_key: string
        }

        resource project_readme from mcp.local.resource.project_readme
        prompt system_prompt from mcp.local.prompt.system_prompt

        tool web_search {
            input {
                query: string
            }

            bindings {
                api_key: secrets.api_key
            }

            output {
                summary: string
            }
        }

        agent researcher {
            model: model.fast
            uses: [tool.web_search]
            instruction: input.topic
            output {
                summary: string
            }
        }
    };

    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);

    assert!(semantic_index.has_provider("openai"));
    assert!(semantic_index.has_model("fast"));
    assert!(semantic_index.has_schema("report"));
    assert!(semantic_index.has_tool("web_search"));
    assert!(semantic_index.has_resource("project_readme"));
    assert!(semantic_index.has_prompt("system_prompt"));
    assert!(semantic_index.has_agent("researcher"));
    assert!(!semantic_index.has_agent("missing"));

    assert_eq!(semantic_index.provider_names().collect::<Vec<_>>(), vec!["openai"]);
    assert_eq!(semantic_index.model_names().collect::<Vec<_>>(), vec!["fast"]);
    assert_eq!(semantic_index.agent_names().collect::<Vec<_>>(), vec!["researcher"]);
}

#[test]
fn workflow_semantic_index_exposes_type_lookup() {
    let workflow = parse_inline_workflow! {
        schema report {
            title: string
        }

        input {
            topic: string
        }

        secrets {
            api_key: string
        }

        tool web_search {
            input {
                query: string
            }

            bindings {
                api_key: secrets.api_key
            }

            output {
                summary: string
            }
        }

        agent researcher {
            uses: [tool.web_search]
            instruction: input.topic
            output {
                summary: string
            }
        }
    };

    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
    let input_field_types = semantic_index
        .input_field_types()
        .expect("input declaration field types should be indexed");
    let secrets_field_types = semantic_index
        .secrets_field_types()
        .expect("secrets declaration field types should be indexed");

    assert_eq!(input_field_types.get("topic"), Some(&TypeExpression::String));
    assert_eq!(secrets_field_types.get("api_key"), Some(&TypeExpression::String));
    assert_eq!(
        semantic_index.tool_input_type("web_search"),
        Some(&WorkflowType::Object(BTreeMap::from([("query".to_owned(), WorkflowType::String)])))
    );
    assert_eq!(
        semantic_index.tool_output_type("web_search"),
        Some(&WorkflowType::Object(BTreeMap::from([(
            "summary".to_owned(),
            WorkflowType::String
        )])))
    );

    let fixed_binding_names = semantic_index
        .tool_fixed_binding_names("web_search")
        .expect("fixed binding names should be indexed");
    let fixed_binding_fields = semantic_index
        .tool_fixed_binding_fields("web_search")
        .expect("fixed binding fields should be indexed");
    let agent_output_type = semantic_index
        .agent_output_type("researcher")
        .expect("agent output type should be indexed")
        .as_ref()
        .expect("agent declares output fields");
    let schema_span = workflow.find_schema("report").expect("schema declaration should exist").span;

    assert!(fixed_binding_names.contains("api_key"));
    assert_eq!(fixed_binding_fields.len(), 1);
    assert!(matches!(agent_output_type, TypeExpression::Object(_)));
    assert!(matches!(
        semantic_index.schema_type_expression("report", schema_span),
        Some(TypeExpression::Object(_))
    ));
}

#[test]
fn workflow_semantic_index_exposes_shared_semantic_summaries() {
    let workflow = parse_inline_workflow! {
        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: secrets.mcp_endpoint
        }

        schema report {
            title: string
            score: number
        }

        input {
            topic: string
        }

        secrets {
            api_key: string
            mcp_endpoint: string
        }

        resource project_readme from mcp.local.resource.project_readme {
            bindings {
                topic: input.topic
            }
        }

        prompt system_prompt from mcp.local.prompt.system_prompt

        tool remote_search from mcp.local.tool.remote_search {
            input {
                query: string
            }

            bindings {
                api_key: secrets.api_key
            }

            output {
                summary: string
            }
        }

        agent researcher {
            model: model.fast
            uses: [tool.remote_search]
            instruction: input.topic
            output {
                summary: string
            }
        }

        output {
            summary: agent.researcher.summary
        }
    };

    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
    let provider = semantic_index.provider("openai").expect("provider summary should be indexed");
    let model = semantic_index.model("fast").expect("model summary should be indexed");
    let mcp_server = semantic_index.mcp_server("local").expect("MCP server summary should be indexed");
    let schema = semantic_index.schema("report").expect("schema summary should be indexed");
    let input_fields = semantic_index.input_fields().expect("input fields should be indexed with spans");
    let tool_schema = semantic_index.tool_schema("remote_search").expect("tool schema should be indexed");
    let tool_import = semantic_index
        .mcp_tool_import("remote_search")
        .expect("MCP tool import should be indexed");
    let resource_import = semantic_index
        .resource_import("project_readme")
        .expect("MCP resource import should be indexed");
    let prompt_import = semantic_index
        .prompt_import("system_prompt")
        .expect("MCP prompt import should be indexed");

    assert_eq!(provider.driver, Some(ProviderDriver::OpenAi));
    assert_eq!(model.provider_name, "openai");
    assert_eq!(model.model_identifier.as_deref(), Some("gpt-4.1-mini"));
    assert_eq!(mcp_server.name, "local");
    assert_eq!(
        schema.fields.get("title").map(|field| &field.field_type),
        Some(&TypeExpression::String)
    );
    assert!(schema.workflow_type.is_some());
    assert_eq!(
        input_fields.get("topic").map(|field| field.span),
        Some(workflow.find_input().expect("input declaration should exist").fields[0].span)
    );
    assert_eq!(
        semantic_index.input_type(),
        Some(&WorkflowType::Object(BTreeMap::from([("topic".to_owned(), WorkflowType::String)])))
    );
    assert_eq!(
        tool_schema.input_fields.get("query").map(|field| &field.field_type),
        Some(&TypeExpression::String)
    );
    assert_eq!(tool_schema.input_type, semantic_index.tool_input_type("remote_search").cloned());
    assert_eq!(tool_schema.output_type, semantic_index.tool_output_type("remote_search").cloned());
    assert_eq!(tool_import.kind, SemanticMcpImportKind::Tool);
    assert_eq!(tool_import.server_name.as_deref(), Some("local"));
    assert_eq!(tool_import.source_name, "remote_search");
    assert_eq!(resource_import.kind, SemanticMcpImportKind::Resource);
    assert_eq!(resource_import.parameters.len(), 1);
    assert_eq!(prompt_import.kind, SemanticMcpImportKind::Prompt);
    assert_eq!(semantic_index.mcp_imports().count(), 3);
    assert!(semantic_index.output_span().is_some());
    assert!(semantic_index.agent_output_workflow_type("researcher").is_some());
}

#[test]
fn validation_uses_promoted_workflow_semantic_index() {
    let workflow = parse_inline_workflow! {
        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1-mini"
        }

        input {
            topic: string
        }

        tool web_search {
            input {
                query: string
            }
        }

        agent researcher {
            model: model.fast
            uses: [tool.web_search]
            instruction: input.topic
            output {
                summary: string
            }
        }

        output {
            summary: agent.researcher.summary
        }
    };

    let validation_report = validate_workflow(&workflow);

    assert!(validation_report.is_valid());
}

#[test]
fn workflow_semantic_index_exposes_core_source_span_lookup_for_declarations() {
    let workflow = parse_inline_workflow! {
        provider openai from openai {}

        model fast from openai {
            id: "gpt-4.1-mini"
        }

        mcp local {
            endpoint: "http://localhost:3000/mcp"
        }

        schema report {
            title: string
        }

        input {
            topic: string
        }

        secrets {
            api_key: string
        }

        resource project_readme from mcp.local.resource.project_readme
        prompt system_prompt from mcp.local.prompt.system_prompt

        tool web_search {
            input {
                query: string
            }

            output {
                summary: string
            }
        }

        agent researcher {
            model: model.fast
            uses: [tool.web_search]
            instruction: input.topic
            output {
                summary: string
            }
        }

        output {
            summary: agent.researcher.summary
        }
    };

    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);

    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::provider("openai")),
        workflow
            .find_provider("openai")
            .map(|provider_declaration| provider_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::model("fast")),
        workflow.find_model("fast").map(|model_declaration| model_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::mcp_server("local")),
        workflow
            .find_mcp_server("local")
            .map(|mcp_server_declaration| mcp_server_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::schema("report")),
        workflow.find_schema("report").map(|schema_declaration| schema_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::input()),
        workflow.find_input().map(|input_declaration| input_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::secrets()),
        workflow.find_secrets().map(|secrets_declaration| secrets_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::tool("web_search")),
        workflow.find_tool("web_search").map(|tool_declaration| tool_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::resource("project_readme")),
        workflow
            .find_resource_import("project_readme")
            .map(|resource_import_declaration| resource_import_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::prompt("system_prompt")),
        workflow
            .find_prompt_import("system_prompt")
            .map(|prompt_import_declaration| prompt_import_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::agent("researcher")),
        workflow.find_agent("researcher").map(|agent_declaration| agent_declaration.span)
    );
    assert_eq!(
        semantic_index.declaration_span(SemanticDeclarationKey::output()),
        workflow.find_output().map(|output_declaration| output_declaration.span)
    );
}

#[test]
fn workflow_semantic_index_exposes_core_source_span_lookup_for_fields() {
    let workflow = source_span_lookup_workflow();
    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
    let schema_declaration = workflow.find_schema("profile").expect("schema should exist");
    let schema_details_field = schema_declaration.fields.first().expect("schema details field should exist");
    let schema_title_field_span = match &schema_details_field.field_type {
        TypeExpression::Object(nested_fields) => nested_fields.first().expect("schema title field should exist").span,
        other_type_expression => panic!("expected nested object field, got {other_type_expression:?}"),
    };
    let input_profile_field_span = workflow.find_input().expect("input declaration should exist").fields[0].span;
    let secret_api_key_field_span = workflow.find_secrets().expect("secrets declaration should exist").fields[0].span;
    let tool_declaration = workflow.find_tool("web_search").expect("tool declaration should exist");

    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::Input, &["profile"]),
        Some(input_profile_field_span)
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::Input, &["profile", "details", "title"]),
        Some(schema_title_field_span)
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::Secrets, &["api_key"]),
        Some(secret_api_key_field_span)
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::Schema("profile"), &["details", "title"]),
        Some(schema_title_field_span)
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::ToolInput("web_search"), &["query"]),
        Some(tool_declaration.input_fields[0].span)
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::ToolOutput("web_search"), &["result", "summary"]),
        Some(match &tool_declaration.output_fields[0].field_type {
            TypeExpression::Object(nested_fields) => nested_fields[0].span,
            other_type_expression => panic!("expected nested object field, got {other_type_expression:?}"),
        })
    );
    assert_eq!(
        semantic_index.field_span(SemanticFieldRoot::AgentOutput("researcher"), &["report", "details", "title"]),
        Some(schema_title_field_span)
    );
}

#[test]
fn workflow_semantic_index_exposes_core_source_span_lookup_for_references() {
    let workflow = source_span_lookup_workflow();
    let semantic_index = WorkflowSemanticIndex::from_workflow(&workflow);
    let schema_declaration = workflow.find_schema("profile").expect("schema should exist");
    let schema_details_field = schema_declaration.fields.first().expect("schema details field should exist");
    let schema_title_field_span = match &schema_details_field.field_type {
        TypeExpression::Object(nested_fields) => nested_fields.first().expect("schema title field should exist").span,
        other_type_expression => panic!("expected nested object field, got {other_type_expression:?}"),
    };
    let tool_declaration = workflow.find_tool("web_search").expect("tool declaration should exist");
    let agent_declaration = workflow.find_agent("researcher").expect("agent declaration should exist");
    let input_reference = Reference {
        root: ReferenceRoot::Keyword(ReferenceKeyword::Input),
        accesses: vec![
            ReferenceAccess::required("profile"),
            ReferenceAccess::required("details"),
            ReferenceAccess::required("title"),
        ],
        span: agent_declaration.span,
    };
    let tool_reference = Reference {
        root: ReferenceRoot::Keyword(ReferenceKeyword::Tool),
        accesses: vec![
            ReferenceAccess::required("web_search"),
            ReferenceAccess::required("result"),
            ReferenceAccess::required("summary"),
        ],
        span: agent_declaration.span,
    };

    assert_eq!(
        semantic_index.reference_definition_span(&input_reference, &ReferenceResolutionScope::new(), 3),
        Some(schema_title_field_span)
    );
    assert_eq!(
        semantic_index.reference_definition_span(&tool_reference, &ReferenceResolutionScope::new(), 1),
        Some(tool_declaration.span)
    );
    assert_eq!(
        semantic_index.reference_definition_span(&tool_reference, &ReferenceResolutionScope::new(), 3),
        Some(match &tool_declaration.output_fields[0].field_type {
            TypeExpression::Object(nested_fields) => nested_fields[0].span,
            other_type_expression => panic!("expected nested object field, got {other_type_expression:?}"),
        })
    );
}

fn source_span_lookup_workflow() -> superwire_core::dsl::Workflow {
    parse_inline_workflow! {
        schema profile {
            details: {
                title: string
            }
        }

        input {
            profile: schema.profile
        }

        secrets {
            api_key: string
        }

        tool web_search {
            input {
                query: string
            }

            bindings {
                api_key: secrets.api_key
            }

            output {
                result: {
                    summary: string
                }
            }
        }

        agent researcher {
            uses: [tool.web_search]
            instruction: input.profile.details.title
            output {
                report: schema.profile
            }
        }
    }
}
