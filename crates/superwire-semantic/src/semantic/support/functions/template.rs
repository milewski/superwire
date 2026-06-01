use super::{BuiltinFunctionHandler, FunctionEvaluationRequest, FunctionTypeInferenceRequest};
use crate::semantic::support::types::WorkflowType;
use crate::semantic::WorkflowSemanticError;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use superwire_types::PromptValueFormat;

pub struct TemplateFunction;

impl BuiltinFunctionHandler for TemplateFunction {
    fn evaluate(&self, request: &FunctionEvaluationRequest<'_>) -> Result<Value, WorkflowSemanticError> {
        if request.function_call.arguments.len() != 2 {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "template(...) requires exactly two arguments: template source and bindings object".to_string(),
            });
        }

        let template_source_expression =
            request
                .function_call
                .argument_expression(0)
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "template(...) first argument is missing".to_string(),
                })?;

        let bindings_expression =
            request
                .function_call
                .argument_expression(1)
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "template(...) second argument is missing".to_string(),
                })?;

        let template_source_value = (request.evaluate_expression)(template_source_expression, request.evaluation_context, request.context)?;

        let Some(template_source_identifier) = template_source_value.as_str() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "template(...) first argument must evaluate to a string".to_string(),
            });
        };

        let bindings_value = (request.evaluate_expression)(bindings_expression, request.evaluation_context, request.context)?;

        let Some(bindings_fields) = bindings_value.as_object() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "template(...) second argument must evaluate to an object".to_string(),
            });
        };

        let template_source = resolve_template_source(template_source_identifier, request.context)?;
        let rendered_template = render_template_with_bindings(&template_source, bindings_fields, request.context)?;

        Ok(Value::String(rendered_template))
    }

    fn infer_type(&self, request: &FunctionTypeInferenceRequest<'_>) -> Result<WorkflowType, WorkflowSemanticError> {
        if request.function_call.arguments.len() != 2 {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: request.context.to_string(),
                message: "template(...) requires exactly two arguments: template source and bindings object".to_string(),
            });
        }

        let template_source_expression =
            request
                .function_call
                .argument_expression(0)
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "template(...) first argument is missing".to_string(),
                })?;

        let bindings_expression =
            request
                .function_call
                .argument_expression(1)
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: request.context.to_string(),
                    message: "template(...) second argument is missing".to_string(),
                })?;

        let _ = (request.infer_expression_type)(template_source_expression, request.type_inference_context, request.context)?;
        let _ = (request.infer_expression_type)(bindings_expression, request.type_inference_context, request.context)?;

        Ok(WorkflowType::String)
    }
}

fn resolve_template_source(template_source_identifier: &str, context: &str) -> Result<String, WorkflowSemanticError> {
    if template_source_identifier.contains("{{") {
        return Ok(template_source_identifier.to_string());
    }

    let direct_template_path = Path::new(template_source_identifier);

    if direct_template_path.exists() {
        return fs::read_to_string(direct_template_path).map_err(|error| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!("failed to read template file `{template_source_identifier}`: {error}"),
        });
    }

    let manifest_relative_template_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(template_source_identifier);

    if manifest_relative_template_path.exists() {
        return fs::read_to_string(&manifest_relative_template_path).map_err(|error| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!(
                "failed to read template file `{}`: {error}",
                manifest_relative_template_path.display()
            ),
        });
    }

    let manifest_workflows_template_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join(template_source_identifier);

    if manifest_workflows_template_path.exists() {
        return fs::read_to_string(&manifest_workflows_template_path).map_err(|error| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!(
                "failed to read template file `{}`: {error}",
                manifest_workflows_template_path.display()
            ),
        });
    }

    let workflow_examples_template_path = Path::new("crates/core/workflows").join(template_source_identifier);

    if workflow_examples_template_path.exists() {
        return fs::read_to_string(&workflow_examples_template_path).map_err(|error| WorkflowSemanticError::ExpressionEvaluation {
            context: context.to_string(),
            message: format!(
                "failed to read template file `{}`: {error}",
                workflow_examples_template_path.display()
            ),
        });
    }

    Ok(template_source_identifier.to_string())
}

fn render_template_with_bindings(
    template_source: &str,
    bindings: &Map<String, Value>,
    context: &str,
) -> Result<String, WorkflowSemanticError> {
    let mut rendered_template = String::new();
    let mut remaining_source = template_source;
    let mut used_binding_names = HashSet::<String>::new();

    loop {
        let Some(interpolation_start_index) = remaining_source.find("{{") else {
            if remaining_source.contains("}}") {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "template source has unmatched closing `}}`".to_string(),
                });
            }

            rendered_template.push_str(remaining_source);
            break;
        };

        rendered_template.push_str(&remaining_source[..interpolation_start_index]);

        let interpolation_source = &remaining_source[interpolation_start_index + 2..];
        let Some(interpolation_end_index) = interpolation_source.find("}}") else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: "template source has unmatched opening `{{`".to_string(),
            });
        };

        let binding_name = interpolation_source[..interpolation_end_index].trim();

        if binding_name.is_empty() {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: "template source contains an empty placeholder".to_string(),
            });
        }

        let Some(binding_value) = bindings.get(binding_name) else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("template binding `{binding_name}` is missing"),
            });
        };

        used_binding_names.insert(binding_name.to_string());
        rendered_template.push_str(&render_template_value(binding_value));

        remaining_source = &interpolation_source[interpolation_end_index + 2..];
    }

    for binding_name in bindings.keys() {
        if !used_binding_names.contains(binding_name) {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("template binding `{binding_name}` is not used in template source"),
            });
        }
    }

    Ok(rendered_template)
}

fn render_template_value(value: &Value) -> String {
    value.to_prompt_text()
}

#[cfg(test)]
mod tests {
    use super::render_template_with_bindings;
    use serde_json::{json, Map};

    #[test]
    fn renders_template_binding_objects_as_prompt_text() {
        let bindings = Map::from_iter([(
            "example".to_string(),
            json!({
                "title": "Demo",
                "count": 2,
                "tags": ["alpha", "beta"]
            }),
        )]);

        assert_eq!(
            render_template_with_bindings("hello world {{ example }}", &bindings, "template test").expect("template should render"),
            "hello world count: 2\ntags:\n- alpha\n- beta\ntitle: Demo"
        );
    }
}
