use crate::ast::{Binding, PromptValue};
use crate::compiler::CompiledWorkflow;
use crate::compiler::TemplateDocument;
use crate::error::WorkflowError;
use crate::runtime::value::{evaluate_plain_expression, render_inline_string, stringify_value};
use crate::runtime::WorkflowState;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn render_prompt(
    prompt: &PromptValue,
    workflow: &CompiledWorkflow,
    state: &WorkflowState,
    local_values: &BTreeMap<String, Value>,
) -> Result<String, WorkflowError> {
    match prompt {
        PromptValue::Inline(template) => render_inline_string(template, workflow, state, local_values),
        PromptValue::Template { path, bindings } => {
            let template_document = TemplateDocument::load(&workflow.base_path, path)?;
            let rendered_bindings = render_bindings(bindings, workflow, state, local_values)?;
            let mut rendered_prompt = template_document.source;

            for (binding_name, binding_value) in rendered_bindings {
                let placeholder = format!("{{{{ {binding_name} }}}}");
                let compact_placeholder = format!("{{{{{binding_name}}}}}");
                rendered_prompt = rendered_prompt.replace(&placeholder, &binding_value);
                rendered_prompt = rendered_prompt.replace(&compact_placeholder, &binding_value);
            }

            Ok(rendered_prompt)
        }
    }
}

fn render_bindings(
    bindings: &[Binding],
    workflow: &CompiledWorkflow,
    state: &WorkflowState,
    local_values: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, String>, WorkflowError> {
    let mut rendered_bindings = BTreeMap::new();

    for binding in bindings {
        let binding_value = evaluate_plain_expression(&binding.value, workflow, state, local_values)?;
        rendered_bindings.insert(binding.name.clone(), stringify_value(&binding_value)?);
    }

    Ok(rendered_bindings)
}

#[cfg(test)]
mod tests {
    use super::render_prompt;
    use crate::ast::{Binding, Expression, PromptValue, StringTemplate, TypeExpression};
    use crate::compiler::{CompiledAgent, CompiledProvider, CompiledWorkflow, DependencyGraph, ProviderDriver};
    use crate::runtime::WorkflowState;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    #[test]
    fn renders_template_prompts_with_stringified_bindings() {
        let workflow = CompiledWorkflow {
            agents: vec![CompiledAgent {
                context: None,
                dependencies: BTreeSet::new(),
                for_loop: None,
                inference: Vec::new(),
                model: crate::ast::ModelSelector {
                    provider_name: "openai".to_string(),
                    model_name: "gpt-4.1-mini".to_string(),
                },
                name: "brief".to_string(),
                output_type: TypeExpression::Primitive(crate::ast::PrimitiveType::String),
                prompt: PromptValue::Inline(StringTemplate {
                    raw: String::new(),
                    fragments: Vec::new(),
                }),
                tools: Vec::new(),
            }],
            base_path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workflows/v2"),
            dependency_graph: DependencyGraph::new(&BTreeMap::from([("brief".to_string(), BTreeSet::new())]))
                .expect("dependency graph should build"),
            input_fields: Vec::new(),
            input_schema: None,
            output_fields: Vec::new(),
            providers: BTreeMap::from([(
                "openai".to_string(),
                CompiledProvider {
                    api_key_secret_name: None,
                    driver: ProviderDriver::OpenAi,
                    endpoint: None,
                    models: vec!["gpt-4.1-mini".to_string()],
                    name: "openai".to_string(),
                },
            )]),
            schemas: BTreeMap::new(),
            secret_fields: Vec::new(),
            secret_schema: None,
        };
        let state = WorkflowState {
            agent_results: BTreeMap::new(),
            inputs: json!({
                "study_name": "DSL",
                "audience": "engineers",
                "findings": ["fast", "typed"],
            }),
            secrets: BTreeMap::new(),
        };
        let prompt = PromptValue::Template {
            path: "prompts/research_brief.md".to_string(),
            bindings: vec![
                Binding {
                    name: "study_name".to_string(),
                    value: Expression::Reference(crate::ast::ReferenceExpression {
                        root: crate::ast::ReferenceRoot::Input("study_name".to_string()),
                        path: Vec::new(),
                    }),
                },
                Binding {
                    name: "audience".to_string(),
                    value: Expression::Reference(crate::ast::ReferenceExpression {
                        root: crate::ast::ReferenceRoot::Input("audience".to_string()),
                        path: Vec::new(),
                    }),
                },
                Binding {
                    name: "findings".to_string(),
                    value: Expression::Reference(crate::ast::ReferenceExpression {
                        root: crate::ast::ReferenceRoot::Input("findings".to_string()),
                        path: Vec::new(),
                    }),
                },
            ],
        };

        let rendered = render_prompt(&prompt, &workflow, &state, &BTreeMap::new()).expect("prompt should render");

        assert!(rendered.contains("Study: DSL"));
        assert!(rendered.contains("Audience: engineers"));
        assert!(rendered.contains("Findings: [\"fast\",\"typed\"]"));
    }
}
