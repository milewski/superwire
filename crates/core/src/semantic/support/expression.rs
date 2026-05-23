use crate::dsl::{
    Asset, AssetPropertyName, Expression, MatchBranch, ModelAssetKind, Reference, ReferenceKeyword, ReferenceRoot, StringTemplatePart,
};
use crate::semantic::support::types::{parse_number_literal, value_kind_name};
use crate::semantic::WorkflowSemanticError;
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub input_values: Map<String, Value>,
    pub secret_values: Map<String, Value>,
    pub agent_outputs: HashMap<String, Value>,
    pub agent_contexts: HashMap<String, Value>,
    pub local_bindings: HashMap<String, Value>,
}

pub fn evaluate_expression(
    expression: &Expression,
    evaluation_context: &EvaluationContext,
    context: &str,
) -> Result<Value, WorkflowSemanticError> {
    expression.evaluate(evaluation_context, context)
}

impl Expression {
    pub fn evaluate(&self, evaluation_context: &EvaluationContext, context: &str) -> Result<Value, WorkflowSemanticError> {
        match self {
            Self::StringLiteral(string_literal) => Ok(Value::String(string_literal.clone())),
            Self::StringTemplate(string_template) => {
                let mut rendered_template = String::new();

                for string_template_part in &string_template.parts {
                    match string_template_part {
                        StringTemplatePart::Text(template_text) => {
                            rendered_template.push_str(template_text);
                        }
                        StringTemplatePart::Interpolation(interpolation_expression) => {
                            let interpolation_value = interpolation_expression.evaluate(evaluation_context, context)?;

                            rendered_template.push_str(&render_template_value(&interpolation_value));
                        }
                    }
                }

                Ok(Value::String(rendered_template))
            }
            Self::NumberLiteral(number_literal) => Ok(Value::Number(parse_number_literal(number_literal)?)),
            Self::BooleanLiteral(boolean_literal) => Ok(Value::Bool(*boolean_literal)),
            Self::NullLiteral => Ok(Value::Null),
            Self::Reference(reference) => reference.evaluate(evaluation_context, context),
            Self::FunctionCall(function_call) => {
                function_call.evaluate_builtin(evaluation_context, context, &|expression, evaluation_context, context| {
                    expression.evaluate(evaluation_context, context)
                })
            }
            Self::Asset(asset) => asset.evaluate(evaluation_context, context),
            Self::ToolCall(_) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "deterministic tool calls must be executed by the workflow runtime".to_string(),
            }),
            Self::McpCall(_) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "MCP resource and prompt calls must be executed by the workflow runtime".to_string(),
            }),
            Self::NullFallback(null_fallback) => {
                let value = null_fallback.value.evaluate(evaluation_context, context)?;

                if value.is_null() {
                    return null_fallback.fallback.evaluate(evaluation_context, context);
                }

                Ok(value)
            }
            Self::VariantProjection(variant_projection) => {
                let value = variant_projection.value.evaluate(evaluation_context, context)?;
                evaluate_variant_projection(value, &variant_projection.case_name, &variant_projection.field_path)
            }
            Self::Match(match_expression) => {
                let value = match_expression.value.evaluate(evaluation_context, context)?;

                for branch in &match_expression.branches {
                    match branch {
                        MatchBranch::Variant {
                            case_name,
                            field_path,
                            span: _,
                        } => {
                            let projected_value = evaluate_variant_projection(value.clone(), case_name, field_path)?;

                            if !projected_value.is_null() {
                                return Ok(projected_value);
                            }
                        }
                        MatchBranch::Fallback { value, span: _ } => return value.evaluate(evaluation_context, context),
                    }
                }

                Ok(Value::Null)
            }
            Self::ArrayLiteral(array_items) => {
                let mut evaluated_items = Vec::with_capacity(array_items.len());

                for array_item in array_items {
                    evaluated_items.push(array_item.evaluate(evaluation_context, context)?);
                }

                Ok(Value::Array(evaluated_items))
            }
            Self::ObjectLiteral(object_fields) => {
                let mut evaluated_fields = Map::new();

                for object_field in object_fields {
                    let field_value = object_field.value.evaluate(evaluation_context, context)?;
                    evaluated_fields.insert(object_field.name.clone(), field_value);
                }

                Ok(Value::Object(evaluated_fields))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetValueField {
    Marker,
    Kind,
    SourceType,
    Url,
    Data,
    MediaType,
    Title,
    Context,
    Citations,
}

impl AssetValueField {
    fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "__superwire_asset",
            Self::Kind => "kind",
            Self::SourceType => "source_type",
            Self::Url => "url",
            Self::Data => "data",
            Self::MediaType => "media_type",
            Self::Title => "title",
            Self::Context => "context",
            Self::Citations => "citations",
        }
    }
}

impl Asset {
    pub fn evaluate(&self, evaluation_context: &EvaluationContext, context: &str) -> Result<Value, WorkflowSemanticError> {
        let source_value = self.source.evaluate(evaluation_context, context)?;
        let Some(source) = source_value.as_str() else {
            return Err(WorkflowSemanticError::ExpressionEvaluation {
                context: context.to_string(),
                message: format!("asset source must resolve to a string, found {}", value_kind_name(&source_value)),
            });
        };

        let mut asset_object = Map::new();
        asset_object.insert(AssetValueField::Marker.as_str().to_string(), Value::Bool(true));

        if let Some((media_type, data)) = Self::split_data_source(source) {
            asset_object.insert(
                AssetValueField::SourceType.as_str().to_string(),
                Value::String("base64".to_string()),
            );
            asset_object.insert(AssetValueField::Data.as_str().to_string(), Value::String(data.to_string()));
            asset_object.insert(
                AssetValueField::MediaType.as_str().to_string(),
                Value::String(media_type.to_string()),
            );
        } else {
            asset_object.insert(AssetValueField::SourceType.as_str().to_string(), Value::String("url".to_string()));
            asset_object.insert(AssetValueField::Url.as_str().to_string(), Value::String(source.to_string()));
        }

        for option in &self.options {
            let option_value = option.value.evaluate(evaluation_context, context)?;

            match AssetPropertyName::from_identifier(option.name.as_str()) {
                Some(AssetPropertyName::Type) => {
                    let Some(kind_name) = option_value.as_str() else {
                        return Err(WorkflowSemanticError::ExpressionEvaluation {
                            context: context.to_string(),
                            message: format!(
                                "asset `type` option must resolve to a string, found {}",
                                value_kind_name(&option_value)
                            ),
                        });
                    };
                    let Some(asset_kind) = ModelAssetKind::from_identifier(kind_name) else {
                        return Err(WorkflowSemanticError::ExpressionEvaluation {
                            context: context.to_string(),
                            message: format!("unsupported asset type `{kind_name}`"),
                        });
                    };

                    asset_object.insert(
                        AssetValueField::Kind.as_str().to_string(),
                        Value::String(asset_kind.as_str().to_string()),
                    );
                }
                Some(AssetPropertyName::MediaType) => {
                    asset_object.insert(AssetValueField::MediaType.as_str().to_string(), option_value);
                }
                Some(AssetPropertyName::Title) => {
                    asset_object.insert(AssetValueField::Title.as_str().to_string(), option_value);
                }
                Some(AssetPropertyName::Context) => {
                    asset_object.insert(AssetValueField::Context.as_str().to_string(), option_value);
                }
                Some(AssetPropertyName::Citations) => {
                    asset_object.insert(AssetValueField::Citations.as_str().to_string(), option_value);
                }
                None => {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("unknown asset option `{}`", option.name),
                    });
                }
            }
        }

        if !asset_object.contains_key(AssetValueField::Kind.as_str()) {
            let asset_kind = asset_object
                .get(AssetValueField::MediaType.as_str())
                .and_then(Value::as_str)
                .and_then(ModelAssetKind::from_media_type)
                .or_else(|| ModelAssetKind::from_source(source))
                .ok_or_else(|| WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: "asset type could not be inferred; set `type` in the asset options".to_string(),
                })?;

            asset_object.insert(
                AssetValueField::Kind.as_str().to_string(),
                Value::String(asset_kind.as_str().to_string()),
            );
        }

        if !asset_object.contains_key(AssetValueField::MediaType.as_str()) {
            if let Some(media_type) = ModelAssetKind::media_type_from_source(source) {
                asset_object.insert(
                    AssetValueField::MediaType.as_str().to_string(),
                    Value::String(media_type.to_string()),
                );
            }
        }

        Ok(Value::Object(asset_object))
    }

    fn split_data_source(source: &str) -> Option<(&str, &str)> {
        let data_source = source.strip_prefix("data:")?;
        let (media_type, data) = data_source.split_once(";base64,")?;

        Some((media_type, data))
    }
}

fn evaluate_variant_projection(value: Value, case_name: &str, field_path: &[String]) -> Result<Value, WorkflowSemanticError> {
    let Some(object_fields) = value.as_object() else {
        return Ok(Value::Null);
    };
    let has_matching_discriminator = object_fields
        .values()
        .any(|field_value| matches!(field_value, Value::String(discriminator_value) if discriminator_value == case_name));

    if !has_matching_discriminator {
        return Ok(Value::Null);
    }

    let Some((first_field_name, remaining_field_path)) = field_path.split_first() else {
        return Ok(value);
    };
    let Some(mut current_value) = object_fields.get(first_field_name) else {
        return Ok(Value::Null);
    };

    for field_name in remaining_field_path {
        let Some(current_object_fields) = current_value.as_object() else {
            return Ok(Value::Null);
        };
        let Some(next_value) = current_object_fields.get(field_name) else {
            return Ok(Value::Null);
        };

        current_value = next_value;
    }

    Ok(current_value.clone())
}

impl Reference {
    pub fn evaluate(&self, evaluation_context: &EvaluationContext, context: &str) -> Result<Value, WorkflowSemanticError> {
        let (mut current_value, access_start_index) = self.resolve_root_value(evaluation_context, context)?;

        for reference_access in self.accesses_from(access_start_index) {
            if current_value.is_null() && reference_access.optional {
                return Ok(Value::Null);
            }

            let Some(object_fields) = current_value.as_object() else {
                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!(
                        "reference path `{}.{}` cannot access field on non-object value",
                        self.render_path(),
                        reference_access.field
                    ),
                });
            };

            let Some(next_value) = object_fields.get(&reference_access.field) else {
                if reference_access.optional {
                    return Ok(Value::Null);
                }

                return Err(WorkflowSemanticError::ExpressionEvaluation {
                    context: context.to_string(),
                    message: format!(
                        "reference path `{}` is missing field `{}`",
                        self.render_path(),
                        reference_access.field
                    ),
                });
            };

            current_value = next_value.clone();
        }

        Ok(current_value)
    }

    fn resolve_root_value(&self, evaluation_context: &EvaluationContext, context: &str) -> Result<(Value, usize), WorkflowSemanticError> {
        match &self.root {
            ReferenceRoot::Keyword(ReferenceKeyword::Input) => {
                if !self.has_accesses() {
                    return Ok((Value::Object(evaluation_context.input_values.clone()), 0));
                }

                let input_field_name = self
                    .first_access_field()
                    .expect("input keyword reference must have first access when not empty");
                let Some(input_field_value) = evaluation_context.input_values.get(input_field_name) else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("unknown input field `{input_field_name}`"),
                    });
                };

                Ok((input_field_value.clone(), 1))
            }
            ReferenceRoot::Keyword(ReferenceKeyword::Dynamic) => {
                if !self.has_accesses() {
                    let dynamic_values = evaluation_context
                        .local_bindings
                        .iter()
                        .map(|(field_name, field_value)| (field_name.clone(), field_value.clone()))
                        .collect::<Map<String, Value>>();

                    return Ok((Value::Object(dynamic_values), 0));
                }

                let dynamic_field_name = self
                    .first_access_field()
                    .expect("dynamic keyword reference must have first access when not empty");
                let Some(dynamic_field_value) = evaluation_context.local_bindings.get(dynamic_field_name) else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("unknown dynamic field `{dynamic_field_name}`"),
                    });
                };

                Ok((dynamic_field_value.clone(), 1))
            }
            ReferenceRoot::Keyword(ReferenceKeyword::Secrets) => {
                if !self.has_accesses() {
                    return Ok((Value::Object(evaluation_context.secret_values.clone()), 0));
                }

                let secret_field_name = self
                    .first_access_field()
                    .expect("secrets keyword reference must have first access when not empty");
                let Some(secret_field_value) = evaluation_context.secret_values.get(secret_field_name) else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("unknown secret field `{secret_field_name}`"),
                    });
                };

                Ok((secret_field_value.clone(), 1))
            }
            ReferenceRoot::Keyword(ReferenceKeyword::Agent) => {
                if !self.has_accesses() {
                    let mut all_agent_outputs = Map::new();

                    for (agent_name, agent_output) in &evaluation_context.agent_outputs {
                        all_agent_outputs.insert(agent_name.clone(), agent_output.clone());
                    }

                    return Ok((Value::Object(all_agent_outputs), 0));
                }

                let agent_name = self
                    .first_access_field()
                    .expect("agent keyword reference must have first access when not empty");
                let Some(agent_output_value) = evaluation_context.agent_outputs.get(agent_name) else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("agent `{agent_name}` output is not available"),
                    });
                };

                Ok((agent_output_value.clone(), 1))
            }
            ReferenceRoot::Keyword(ReferenceKeyword::Tool) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`tool.*` runtime references are not yet supported".to_string(),
            }),
            ReferenceRoot::Keyword(ReferenceKeyword::Resource) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`resource.*` runtime references are not supported outside `read resource.*`".to_string(),
            }),
            ReferenceRoot::Keyword(ReferenceKeyword::Prompt) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`prompt.*` runtime references are not supported outside `render prompt.*`".to_string(),
            }),
            ReferenceRoot::Keyword(ReferenceKeyword::Model) => Err(WorkflowSemanticError::UnsupportedFeature {
                feature: "`model.*` references are only supported in agent model properties".to_string(),
            }),
            ReferenceRoot::Identifier(identifier) => {
                let Some(local_binding_value) = evaluation_context.local_bindings.get(identifier) else {
                    return Err(WorkflowSemanticError::ExpressionEvaluation {
                        context: context.to_string(),
                        message: format!("unknown identifier `{identifier}`"),
                    });
                };

                Ok((local_binding_value.clone(), 0))
            }
        }
    }
}

fn render_template_value(value: &Value) -> String {
    if value.is_superwire_asset() {
        return String::new();
    }

    if let Some(string_value) = value.as_str() {
        return string_value.to_string();
    }

    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

trait ValueAssetExt {
    fn is_superwire_asset(&self) -> bool;
}

impl ValueAssetExt for Value {
    fn is_superwire_asset(&self) -> bool {
        self.get(AssetValueField::Marker.as_str()).and_then(Value::as_bool) == Some(true)
    }
}
