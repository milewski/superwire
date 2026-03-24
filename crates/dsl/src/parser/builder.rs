use crate::ast::{
    AccessOperator, AgentDeclaration, AgentProperty, Binding, CompactArgument, CompactExpression, Expression, ForLoopBinding,
    FunctionExpression, InferenceProperty, ModelSelector, ObjectField, PrimitiveType, PromptValue, ProviderDeclaration, ProviderProperty,
    ReferenceExpression, ReferenceRoot, StringTemplate, ToolUsage, TypeExpression, TypeField, Workflow,
};
use crate::error::WorkflowError;
use crate::parser::grammar::Rule;
use crate::parser::string::parse_string_template;
use crate::parser::visitor::GrammarVisitor;
use crate::parser::WorkflowParser;
use pest::iterators::Pair;
use pest::Parser;

pub fn parse_workflow(source: &str) -> Result<Workflow, WorkflowError> {
    let mut pairs = WorkflowParser::parse(Rule::workflow, source).map_err(|error| WorkflowError::parse(error.to_string()))?;
    let workflow_pair = pairs.next().expect("workflow parse should always return a root pair");

    AstBuilder.visit_workflow(workflow_pair)
}

pub(crate) struct AstBuilder;

impl AstBuilder {
    pub(crate) fn build_reference_expression(pair: Pair<'_, Rule>) -> Result<ReferenceExpression, WorkflowError> {
        if matches!(
            pair.as_rule(),
            Rule::agent_reference | Rule::input_reference | Rule::local_reference | Rule::secret_reference | Rule::reference_root
        ) {
            return Ok(ReferenceExpression {
                root: Self::visit_reference_root(pair)?,
                path: Vec::new(),
            });
        }

        let mut inner_pairs = pair.into_inner();
        let root_pair = inner_pairs.next().expect("reference expression should include a root");
        let root = Self::visit_reference_root(root_pair)?;
        let path = inner_pairs.map(Self::visit_path_segment).collect::<Result<Vec<_>, _>>()?;

        Ok(ReferenceExpression { root, path })
    }

    fn consume_string(pair: Pair<'_, Rule>) -> Result<StringTemplate, WorkflowError> {
        if pair.as_rule() == Rule::description {
            return Self::consume_string(pair.into_inner().next().expect("description should contain a string literal"));
        }

        match pair.as_rule() {
            Rule::multiline_string => parse_string_template(pair.as_str(), true),
            Rule::string_literal => parse_string_template(pair.as_str(), false),
            _ => unreachable!("unexpected rule for string consumption: {:?}", pair.as_rule()),
        }
    }

    fn parse_usize(value: &str) -> Result<usize, WorkflowError> {
        value
            .replace('_', "")
            .parse::<usize>()
            .map_err(|error| WorkflowError::parse(format!("failed to parse integer '{value}': {error}")))
    }

    fn parse_u32(value: &str) -> Result<u32, WorkflowError> {
        value
            .replace('_', "")
            .parse::<u32>()
            .map_err(|error| WorkflowError::parse(format!("failed to parse integer '{value}': {error}")))
    }

    fn parse_i32(value: &str) -> Result<i32, WorkflowError> {
        value
            .replace('_', "")
            .parse::<i32>()
            .map_err(|error| WorkflowError::parse(format!("failed to parse integer '{value}': {error}")))
    }

    fn visit_statement(&mut self, pair: Pair<'_, Rule>, workflow: &mut Workflow) -> Result<(), WorkflowError> {
        match pair.as_rule() {
            Rule::agent_decl => workflow.agents.push(self.visit_agent_declaration(pair)?),
            Rule::EOI => {}
            Rule::input_decl => {
                workflow.input_fields = self.visit_typed_field_block(pair.into_inner().next().expect("input should have a block"))?;
            }
            Rule::output_decl => {
                let output_pair = pair.into_inner().next().expect("output should have an object block");
                workflow.output_fields = self.visit_object_block(output_pair)?;
            }
            Rule::provider_decl => workflow.providers.push(self.visit_provider_declaration(pair)?),
            Rule::schema_decl => {
                let mut inner_pairs = pair.into_inner();
                let schema_name = inner_pairs.next().expect("schema should have a name").as_str().to_string();
                let schema_fields = self.visit_typed_field_block(inner_pairs.next().expect("schema should have a block"))?;
                workflow.schemas.push((schema_name, schema_fields));
            }
            Rule::secrets_decl => {
                let secrets_block = pair.into_inner().next().expect("secrets should have a block");
                workflow.secret_fields = self.visit_typed_field_block(secrets_block)?;
            }
            _ => unreachable!("unexpected statement rule: {:?}", pair.as_rule()),
        }

        Ok(())
    }

    fn visit_provider_declaration(&mut self, pair: Pair<'_, Rule>) -> Result<ProviderDeclaration, WorkflowError> {
        let mut inner_pairs = pair.into_inner();
        let name = inner_pairs.next().expect("provider should have a name").as_str().to_string();
        let provider_block = inner_pairs.next().expect("provider should have a block");
        let properties = provider_block
            .into_inner()
            .map(|property_pair| match property_pair.as_rule() {
                Rule::provider_api_key => {
                    let secret_pair = property_pair
                        .into_inner()
                        .next()
                        .expect("api_key should contain a secret reference");
                    Ok::<ProviderProperty, WorkflowError>(ProviderProperty::ApiKey(Expression::Reference(
                        Self::build_reference_expression(secret_pair)?,
                    )))
                }
                Rule::provider_driver => {
                    let driver_pair = property_pair.into_inner().next().expect("driver should contain a string");
                    let driver = Self::consume_string(driver_pair)?;
                    Ok::<ProviderProperty, WorkflowError>(ProviderProperty::Driver(driver.raw))
                }
                Rule::provider_endpoint => {
                    let endpoint_pair = property_pair.into_inner().next().expect("endpoint should contain a string");
                    let endpoint = Self::consume_string(endpoint_pair)?;
                    Ok::<ProviderProperty, WorkflowError>(ProviderProperty::Endpoint(endpoint.raw))
                }
                Rule::provider_models => {
                    let models = property_pair
                        .into_inner()
                        .next()
                        .expect("models should contain an array")
                        .into_inner()
                        .map(|string_pair| Self::consume_string(string_pair).map(|string_value| string_value.raw))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok::<ProviderProperty, WorkflowError>(ProviderProperty::Models(models))
                }
                _ => unreachable!("unexpected provider property rule: {:?}", property_pair.as_rule()),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ProviderDeclaration { name, properties })
    }

    fn visit_agent_declaration(&mut self, pair: Pair<'_, Rule>) -> Result<AgentDeclaration, WorkflowError> {
        let mut inner_pairs = pair.into_inner().peekable();
        let name = inner_pairs.next().expect("agent should have a name").as_str().to_string();
        let mut for_loop = None;

        if inner_pairs.peek().is_some_and(|pair| pair.as_rule() == Rule::for_clause) {
            for_loop = Some(self.visit_for_clause(inner_pairs.next().expect("for clause should be present"))?);
        }

        let properties = inner_pairs
            .next()
            .expect("agent should have a block")
            .into_inner()
            .map(|property_pair| self.visit_agent_property(property_pair))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AgentDeclaration {
            name,
            for_loop,
            properties,
        })
    }

    fn visit_for_clause(&mut self, pair: Pair<'_, Rule>) -> Result<ForLoopBinding, WorkflowError> {
        let mut inner_pairs = pair.into_inner();
        let item_name = inner_pairs
            .next()
            .expect("for clause should include an item name")
            .as_str()
            .to_string();
        let source = self.visit_expression(inner_pairs.next().expect("for clause should include a source"))?;

        Ok(ForLoopBinding { item_name, source })
    }

    fn visit_agent_property(&mut self, pair: Pair<'_, Rule>) -> Result<AgentProperty, WorkflowError> {
        match pair.as_rule() {
            Rule::agent_context => Ok(AgentProperty::Context(
                self.visit_expression(pair.into_inner().next().expect("context property should have a value"))?,
            )),
            Rule::agent_inference => {
                let inference_pair = pair.into_inner().next().expect("inference property should contain a block");
                Ok(AgentProperty::Inference(self.visit_inference_block(inference_pair)?))
            }
            Rule::agent_model => {
                let model_pair = pair.into_inner().next().expect("model property should contain a selector");
                Ok(AgentProperty::Model(self.visit_model_selector(model_pair)?))
            }
            Rule::agent_output => {
                let output_pair = pair.into_inner().next().expect("output property should contain a type");
                Ok(AgentProperty::Output(self.visit_type_expression(output_pair)?))
            }
            Rule::agent_prompt => {
                let prompt_pair = pair.into_inner().next().expect("prompt property should contain a value");
                Ok(AgentProperty::Prompt(self.visit_prompt_value(prompt_pair)?))
            }
            Rule::agent_tools => {
                let tool_array = pair.into_inner().next().expect("tools property should contain an array");
                let tools = tool_array
                    .into_inner()
                    .map(|tool_pair| self.visit_tool_usage(tool_pair))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(AgentProperty::Tools(tools))
            }
            _ => unreachable!("unexpected agent property rule: {:?}", pair.as_rule()),
        }
    }

    fn visit_tool_usage(&mut self, pair: Pair<'_, Rule>) -> Result<ToolUsage, WorkflowError> {
        let mut inner_pairs = pair.into_inner();
        let tool_reference = inner_pairs.next().expect("tool usage should include a tool reference");
        let name = tool_reference
            .into_inner()
            .next()
            .expect("tool reference should include an identifier")
            .as_str()
            .to_string();

        let arguments = if let Some(arguments_pair) = inner_pairs.next() {
            arguments_pair
                .into_inner()
                .map(|argument_pair| self.visit_binding(argument_pair))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        Ok(ToolUsage { name, arguments })
    }

    fn visit_binding(&mut self, pair: Pair<'_, Rule>) -> Result<Binding, WorkflowError> {
        let mut inner_pairs = pair.into_inner();
        let name = inner_pairs.next().expect("binding should include a name").as_str().to_string();
        let value = self.visit_expression(inner_pairs.next().expect("binding should include a value"))?;

        Ok(Binding { name, value })
    }

    fn visit_prompt_value(&mut self, pair: Pair<'_, Rule>) -> Result<PromptValue, WorkflowError> {
        match pair.as_rule() {
            Rule::multiline_string | Rule::string_literal => Ok(PromptValue::Inline(Self::consume_string(pair)?)),
            Rule::template_call => {
                let mut inner_pairs = pair.into_inner();
                let path = Self::consume_string(inner_pairs.next().expect("template should include a path"))?.raw;
                let bindings = inner_pairs
                    .next()
                    .expect("template should include a binding block")
                    .into_inner()
                    .map(|binding_pair| self.visit_binding(binding_pair))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(PromptValue::Template { path, bindings })
            }
            _ => unreachable!("unexpected prompt value rule: {:?}", pair.as_rule()),
        }
    }

    fn visit_model_selector(&mut self, pair: Pair<'_, Rule>) -> Result<ModelSelector, WorkflowError> {
        let mut inner_pairs = pair.into_inner();
        let provider_name = inner_pairs
            .next()
            .expect("model selector should include a provider name")
            .as_str()
            .to_string();
        let model_name = Self::consume_string(inner_pairs.next().expect("model selector should include a model name"))?.raw;

        Ok(ModelSelector { provider_name, model_name })
    }

    fn visit_inference_block(&mut self, pair: Pair<'_, Rule>) -> Result<Vec<InferenceProperty>, WorkflowError> {
        pair.into_inner()
            .map(|property_pair| {
                let property = match property_pair.as_rule() {
                    Rule::inference_frequency_penalty => InferenceProperty::FrequencyPenalty(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("frequency_penalty should have a number")
                            .as_str()
                            .to_string(),
                    ),
                    Rule::inference_max_tokens => InferenceProperty::MaxTokens(Self::parse_usize(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("max_tokens should have an integer")
                            .as_str(),
                    )?),
                    Rule::inference_presence_penalty => InferenceProperty::PresencePenalty(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("presence_penalty should have a number")
                            .as_str()
                            .to_string(),
                    ),
                    Rule::inference_repeat_penalty => InferenceProperty::RepeatPenalty(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("repeat_penalty should have a number")
                            .as_str()
                            .to_string(),
                    ),
                    Rule::inference_seed => InferenceProperty::Seed(Self::parse_i32(
                        property_pair.into_inner().next().expect("seed should have an integer").as_str(),
                    )?),
                    Rule::inference_stop_sequences => InferenceProperty::StopSequences(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("stop_sequences should include an array")
                            .into_inner()
                            .map(|string_pair| Self::consume_string(string_pair).map(|string_value| string_value.raw))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    Rule::inference_temperature => InferenceProperty::Temperature(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("temperature should have a number")
                            .as_str()
                            .to_string(),
                    ),
                    Rule::inference_top_k => InferenceProperty::TopK(Self::parse_u32(
                        property_pair.into_inner().next().expect("top_k should have an integer").as_str(),
                    )?),
                    Rule::inference_top_p => InferenceProperty::TopP(
                        property_pair
                            .into_inner()
                            .next()
                            .expect("top_p should have a number")
                            .as_str()
                            .to_string(),
                    ),
                    _ => unreachable!("unexpected inference property rule: {:?}", property_pair.as_rule()),
                };

                Ok(property)
            })
            .collect()
    }

    fn visit_typed_field_block(&mut self, pair: Pair<'_, Rule>) -> Result<Vec<TypeField>, WorkflowError> {
        pair.into_inner()
            .map(|field_pair| {
                let mut inner_pairs = field_pair.into_inner();
                let name = inner_pairs.next().expect("typed field should include a name").as_str().to_string();
                let value_type = self.visit_type_expression(inner_pairs.next().expect("typed field should include a type"))?;
                let description = inner_pairs
                    .next()
                    .map(Self::consume_string)
                    .transpose()?
                    .map(|string_value| string_value.raw);

                Ok(TypeField {
                    name,
                    value_type,
                    description,
                })
            })
            .collect()
    }

    fn visit_type_expression(&mut self, pair: Pair<'_, Rule>) -> Result<TypeExpression, WorkflowError> {
        match pair.as_rule() {
            Rule::type_expression => self.visit_type_expression(pair.into_inner().next().expect("type_expression should contain a union")),
            Rule::union_type => {
                let union_members = pair
                    .into_inner()
                    .map(|inner_pair| self.visit_type_expression(inner_pair))
                    .collect::<Result<Vec<_>, _>>()?;

                if union_members.len() == 1 {
                    Ok(union_members.into_iter().next().expect("single union member should exist"))
                } else {
                    Ok(TypeExpression::Union(union_members))
                }
            }
            Rule::array_type => Ok(TypeExpression::Array(Box::new(
                self.visit_type_expression(pair.into_inner().next().expect("array type should contain an inner type"))?,
            ))),
            Rule::fixed_array_type => {
                let mut inner_pairs = pair.into_inner();
                let item_type = self.visit_type_expression(inner_pairs.next().expect("fixed array should contain an item type"))?;
                let length = Self::parse_usize(inner_pairs.next().expect("fixed array should contain a length").as_str())?;

                Ok(TypeExpression::FixedArray {
                    item_type: Box::new(item_type),
                    length,
                })
            }
            Rule::named_schema_ref => Ok(TypeExpression::NamedSchema(
                pair.into_inner()
                    .next()
                    .expect("named schema ref should contain a name")
                    .as_str()
                    .to_string(),
            )),
            Rule::null_type => Ok(TypeExpression::Null),
            Rule::object_type => Ok(TypeExpression::Object(
                self.visit_typed_field_block(pair.into_inner().next().expect("object type should contain a field block"))?,
            )),
            Rule::typed_field_block => Ok(TypeExpression::Object(self.visit_typed_field_block(pair)?)),
            Rule::parenthesized_type => {
                self.visit_type_expression(pair.into_inner().next().expect("parenthesized type should contain an inner type"))
            }
            Rule::primitive_type => {
                let primitive_type = match pair.as_str() {
                    "boolean" => PrimitiveType::Boolean,
                    "float" => PrimitiveType::Float,
                    "number" => PrimitiveType::Number,
                    "string" => PrimitiveType::String,
                    _ => unreachable!("unexpected primitive type literal: {}", pair.as_str()),
                };

                Ok(TypeExpression::Primitive(primitive_type))
            }
            Rule::string_literal => Ok(TypeExpression::StringLiteral(Self::consume_string(pair)?.raw)),
            Rule::tuple_type => Ok(TypeExpression::Tuple(
                pair.into_inner()
                    .map(|inner_pair| self.visit_type_expression(inner_pair))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => unreachable!("unexpected type expression rule: {:?}", pair.as_rule()),
        }
    }

    fn visit_object_block(&mut self, pair: Pair<'_, Rule>) -> Result<Vec<ObjectField>, WorkflowError> {
        pair.into_inner()
            .map(|field_pair| {
                let mut inner_pairs = field_pair.into_inner();
                let name = inner_pairs.next().expect("object field should include a name").as_str().to_string();
                let value = self.visit_expression(inner_pairs.next().expect("object field should include a value"))?;

                Ok(ObjectField { name, value })
            })
            .collect()
    }

    fn visit_expression(&mut self, pair: Pair<'_, Rule>) -> Result<Expression, WorkflowError> {
        match pair.as_rule() {
            Rule::array_expression => Ok(Expression::Array(
                pair.into_inner()
                    .map(|inner_pair| self.visit_expression(inner_pair))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Rule::boolean_literal => Ok(Expression::Boolean(pair.as_str() == "true")),
            Rule::compact_call => Ok(Expression::Function(FunctionExpression::Compact(
                self.visit_compact_expression(pair)?,
            ))),
            Rule::context_call => {
                let reference_pair = pair.into_inner().next().expect("context call should include an agent reference");
                Ok(Expression::Function(FunctionExpression::Context(Self::build_reference_expression(
                    reference_pair,
                )?)))
            }
            Rule::function_expression => {
                self.visit_expression(pair.into_inner().next().expect("function expression should have inner expression"))
            }
            Rule::multiline_string | Rule::string_literal => Ok(Expression::String(Self::consume_string(pair)?)),
            Rule::null_literal => Ok(Expression::Null),
            Rule::number_literal => Ok(Expression::Number(pair.as_str().to_string())),
            Rule::object_block => Ok(Expression::Object(self.visit_object_block(pair)?)),
            Rule::object_expression => Ok(Expression::Object(
                self.visit_object_block(pair.into_inner().next().expect("object expression should contain an object block"))?,
            )),
            Rule::reference_expression => Ok(Expression::Reference(Self::build_reference_expression(pair)?)),
            Rule::value_expression => self.visit_expression(
                pair.into_inner()
                    .next()
                    .expect("value expression should contain an inner expression"),
            ),
            _ => unreachable!("unexpected expression rule: {:?}", pair.as_rule()),
        }
    }

    fn visit_compact_expression(&mut self, pair: Pair<'_, Rule>) -> Result<CompactExpression, WorkflowError> {
        let Some(arguments_pair) = pair.into_inner().next() else {
            return Ok(CompactExpression { arguments: Vec::new() });
        };

        if arguments_pair.as_rule() == Rule::compact_arguments {
            return self.visit_compact_expression(arguments_pair);
        }

        let arguments = match arguments_pair.as_rule() {
            Rule::compact_named_arguments | Rule::compact_positional_arguments => arguments_pair
                .into_inner()
                .map(|argument_pair| self.visit_compact_argument(argument_pair))
                .collect::<Result<Vec<_>, _>>()?,
            _ => unreachable!("unexpected compact arguments rule: {:?}", arguments_pair.as_rule()),
        };

        Ok(CompactExpression { arguments })
    }

    fn visit_compact_argument(&mut self, pair: Pair<'_, Rule>) -> Result<CompactArgument, WorkflowError> {
        match pair.as_rule() {
            Rule::agent_reference => Ok(CompactArgument::Agent(Self::build_reference_expression(pair)?)),
            Rule::compact_agent_argument => Ok(CompactArgument::Agent(Self::build_reference_expression(
                pair.into_inner().next().expect("compact agent argument should include a reference"),
            )?)),
            Rule::compact_inference_argument => Ok(CompactArgument::Inference(
                self.visit_inference_block(pair.into_inner().next().expect("compact inference argument should include a block"))?,
            )),
            Rule::compact_model_argument => Ok(CompactArgument::Model(
                self.visit_model_selector(pair.into_inner().next().expect("compact model argument should include a selector"))?,
            )),
            Rule::compact_prompt_argument => Ok(CompactArgument::Prompt(Self::consume_string(
                pair.into_inner().next().expect("compact prompt argument should include a string"),
            )?)),
            _ => unreachable!("unexpected compact argument rule: {:?}", pair.as_rule()),
        }
    }

    fn visit_reference_root(pair: Pair<'_, Rule>) -> Result<ReferenceRoot, WorkflowError> {
        let rule = pair.as_rule();

        if rule == Rule::reference_root {
            return Self::visit_reference_root(pair.into_inner().next().expect("reference root should contain a concrete root"));
        }

        let pair_text = pair.as_str().to_string();
        let mut inner_pairs = pair.into_inner();

        match rule {
            Rule::agent_reference => Ok(ReferenceRoot::Agent(
                inner_pairs
                    .next()
                    .expect("agent reference should include a name")
                    .as_str()
                    .to_string(),
            )),
            Rule::input_reference => Ok(ReferenceRoot::Input(
                inner_pairs
                    .next()
                    .expect("input reference should include a name")
                    .as_str()
                    .to_string(),
            )),
            Rule::local_reference => Ok(ReferenceRoot::Local(pair_text)),
            Rule::secret_reference => Ok(ReferenceRoot::Secrets(
                inner_pairs
                    .next()
                    .expect("secret reference should include a name")
                    .as_str()
                    .to_string(),
            )),
            _ => unreachable!("unexpected reference root rule: {:?}", rule),
        }
    }

    fn visit_path_segment(pair: Pair<'_, Rule>) -> Result<crate::ast::PathSegment, WorkflowError> {
        if pair.as_rule() == Rule::path_segment {
            return Self::visit_path_segment(pair.into_inner().next().expect("path segment should contain a concrete segment"));
        }

        match pair.as_rule() {
            Rule::direct_access_segment => Ok(crate::ast::PathSegment {
                operator: AccessOperator::Direct,
                property_name: pair
                    .into_inner()
                    .next()
                    .expect("direct access should include a property")
                    .as_str()
                    .to_string(),
            }),
            Rule::safe_access_segment => Ok(crate::ast::PathSegment {
                operator: AccessOperator::Safe,
                property_name: pair
                    .into_inner()
                    .next()
                    .expect("safe access should include a property")
                    .as_str()
                    .to_string(),
            }),
            _ => unreachable!("unexpected path segment rule: {:?}", pair.as_rule()),
        }
    }
}

impl GrammarVisitor for AstBuilder {
    fn visit_workflow(&mut self, pair: Pair<'_, Rule>) -> Result<Workflow, WorkflowError> {
        let mut workflow = Workflow {
            agents: Vec::new(),
            input_fields: Vec::new(),
            output_fields: Vec::new(),
            providers: Vec::new(),
            schemas: Vec::new(),
            secret_fields: Vec::new(),
        };

        for statement_pair in pair.into_inner() {
            self.visit_statement(statement_pair, &mut workflow)?;
        }

        Ok(workflow)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_workflow;

    #[test]
    fn parses_minimal_workflow_shape() {
        let workflow = parse_workflow(
            r#"
            provider ollama {
                driver: "ollama"
                models: ["qwen3.5:32b"]
            }

            agent greeting {
                model: ollama("qwen3.5:32b")
                prompt: "Write a short welcome message."
                output: string
            }

            output {
                greeting: agent.greeting
            }
            "#,
        )
        .expect("workflow should parse");

        assert_eq!(workflow.agents.len(), 1);
        assert_eq!(workflow.providers.len(), 1);
        assert_eq!(workflow.output_fields.len(), 1);
    }

    #[test]
    fn parses_multiline_string_prompt() {
        let workflow = parse_workflow(
            r#"
            provider ollama {
                driver: "ollama"
                models: ["qwen3.5:8b"]
            }

            agent greeting {
                model: ollama("qwen3.5:8b")
                prompt: """
                    Hello {{ input.name }}
                    Welcome aboard.
                """
                output: string
            }

            output {
                greeting: agent.greeting
            }
            "#,
        )
        .expect("workflow should parse");

        let prompt = match &workflow.agents[0].properties[1] {
            crate::ast::AgentProperty::Prompt(crate::ast::PromptValue::Inline(prompt)) => prompt,
            _ => panic!("expected inline prompt"),
        };

        assert!(prompt.raw.contains("Hello"));
        assert_eq!(prompt.fragments.len(), 3);
    }
}
