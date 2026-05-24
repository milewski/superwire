use super::{source_span_from_pair, AstVisitor};
use crate::dsl::ast::{
    AgentContext, AgentContextReference, AgentDeclaration, AgentForLoop, AgentForLoopPattern, AgentProperty, CompactAgentContext,
    Declaration, DynamicBlock, Expression, ModelUsage, SourceSpan, ToolCall, ToolCallPropertyName,
};
use crate::dsl::parser::{DslParseError, Rule};
use crate::dsl::structure;
use pest::iterators::Pair;

impl AstVisitor {
    pub(super) fn visit_agent_declaration(&self, agent_pair: Pair<'_, Rule>) -> Result<Declaration, DslParseError> {
        let declaration_span = source_span_from_pair(&agent_pair);
        let mut inner_pairs = agent_pair.into_inner();

        let agent_name = self.next_identifier(&mut inner_pairs, "agent name", "agent declaration")?;
        let mut for_loop: Option<AgentForLoop> = None;
        let mut properties = Vec::new();

        for inner_pair in inner_pairs {
            match inner_pair.as_rule() {
                Rule::for_clause => {
                    for_loop = Some(self.visit_for_clause(inner_pair)?);
                }
                Rule::agent_block => {
                    properties = self.visit_agent_block(inner_pair)?;
                }
                _ => unreachable!("agent declaration should include for clause or block"),
            }
        }

        Ok(Declaration::Agent(AgentDeclaration {
            name: agent_name,
            for_loop,
            properties,
            span: declaration_span,
        }))
    }

    pub(super) fn visit_for_clause(&self, for_clause_pair: Pair<'_, Rule>) -> Result<AgentForLoop, DslParseError> {
        let mut inner_pairs = for_clause_pair.into_inner();

        let pattern_pair = self.next_pair(&mut inner_pairs, "for-loop pattern", "for clause")?;
        let pattern = self.visit_for_loop_pattern(pattern_pair)?;
        let iterable_pair = self.next_pair(&mut inner_pairs, "iterable expression", "for clause")?;
        let iterable = self.visit_expression(iterable_pair)?;

        Ok(AgentForLoop { pattern, iterable })
    }

    pub(super) fn visit_for_loop_pattern(&self, pattern_pair: Pair<'_, Rule>) -> Result<AgentForLoopPattern, DslParseError> {
        match pattern_pair.as_rule() {
            Rule::for_loop_pattern => {
                let inner_pattern_pair = self.first_inner_pair(pattern_pair, "for-loop pattern")?;

                self.visit_for_loop_pattern(inner_pattern_pair)
            }
            Rule::identifier => Ok(AgentForLoopPattern::Identifier(pattern_pair.as_str().to_owned())),
            Rule::object_destructuring_pattern => {
                let mut field_names = Vec::new();

                for identifier_pair in pattern_pair.into_inner() {
                    if identifier_pair.as_rule() != Rule::identifier {
                        return Err(DslParseError::unexpected_with_span(
                            identifier_pair.as_rule(),
                            "object destructuring pattern",
                            source_span_from_pair(&identifier_pair),
                        ));
                    }

                    field_names.push(identifier_pair.as_str().to_owned());
                }

                Ok(AgentForLoopPattern::ObjectDestructuring(field_names))
            }
            _ => Err(DslParseError::unexpected_with_span(
                pattern_pair.as_rule(),
                "for-loop pattern",
                source_span_from_pair(&pattern_pair),
            )),
        }
    }

    pub(super) fn visit_agent_block(&self, agent_block_pair: Pair<'_, Rule>) -> Result<Vec<AgentProperty>, DslParseError> {
        let mut properties = Vec::new();

        for property_pair in agent_block_pair.into_inner() {
            properties.push(self.visit_agent_property(property_pair)?);
        }

        Ok(properties)
    }

    pub(super) fn visit_agent_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let property_span = source_span_from_pair(&property_pair);

        match property_pair.as_rule() {
            Rule::model_agent_property => self.visit_agent_model_property(property_pair),
            Rule::agent_output_property => self.visit_agent_output_property(property_pair),
            Rule::named_agent_context_property => self.visit_agent_context_property(property_pair),
            Rule::named_object_property => self.visit_agent_object_property(property_pair, property_span),
            Rule::named_agent_value_property => self.visit_agent_value_property(property_pair, property_span),
            _ => unreachable!("agent block should contain only valid agent property rules"),
        }
    }

    pub(super) fn visit_agent_model_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let mut inner_pairs = property_pair.into_inner();
        let model_usage_pair = self.next_pair(&mut inner_pairs, "agent model value", "agent model property")?;
        let model_usage = self.visit_model_usage(model_usage_pair)?;

        Ok(AgentProperty::Model(model_usage))
    }

    pub(super) fn visit_model_usage(&self, model_usage_pair: Pair<'_, Rule>) -> Result<ModelUsage, DslParseError> {
        let model_usage_span = source_span_from_pair(&model_usage_pair);
        let mut inner_pairs = model_usage_pair.into_inner();
        let reference_pair = self.next_pair(&mut inner_pairs, "model reference", "model usage")?;
        let reference = self.visit_reference(reference_pair)?;

        let properties = if let Some(block_pair) = inner_pairs.next() {
            self.visit_config_block(block_pair)?
        } else {
            Vec::new()
        };

        Ok(ModelUsage {
            reference,
            properties,
            span: model_usage_span,
        })
    }

    pub(super) fn visit_agent_object_property(
        &self,
        property_pair: Pair<'_, Rule>,
        property_span: SourceSpan,
    ) -> Result<AgentProperty, DslParseError> {
        let mut inner_pairs = property_pair.into_inner();
        let property_name = self.next_identifier(&mut inner_pairs, "agent property name", "agent object property")?;
        let object_expression_pair = self.next_pair(&mut inner_pairs, "agent object property value", "agent object property")?;

        let agent = structure::Agent::new();

        if agent.property_is_dynamic(property_name.as_str()) {
            return Ok(AgentProperty::Dynamic(DynamicBlock {
                fields: self.visit_object_expression(object_expression_pair)?,
                span: property_span,
            }));
        }

        if agent.property_is_output(property_name.as_str()) {
            return Err(DslParseError::unexpected_with_span(
                Rule::named_object_property,
                "agent output property",
                property_span,
            ));
        }

        if agent.property_definition(property_name.as_str()).is_some() {
            return Err(DslParseError::unexpected_with_span(
                Rule::named_object_property,
                "agent object property",
                property_span,
            ));
        }

        Ok(AgentProperty::Unknown {
            name: property_name,
            span: property_span,
        })
    }

    pub(super) fn visit_agent_value_property(
        &self,
        property_pair: Pair<'_, Rule>,
        property_span: SourceSpan,
    ) -> Result<AgentProperty, DslParseError> {
        let mut inner_pairs = property_pair.into_inner();
        let property_name = self.next_identifier(&mut inner_pairs, "agent property name", "agent value property")?;
        let agent = structure::Agent::new();

        if agent.property_definition(property_name.as_str()).is_none() {
            return Ok(AgentProperty::Unknown {
                name: property_name,
                span: property_span,
            });
        }

        let value_pair = self.next_pair(&mut inner_pairs, "agent property value", "agent value property")?;

        if agent.property_is_model(property_name.as_str()) {
            return Ok(AgentProperty::InvalidModel(self.visit_expression(value_pair)?));
        }

        if agent.property_is_instruction(property_name.as_str()) {
            return Ok(AgentProperty::Instruction(self.visit_expression(value_pair)?));
        }

        if agent.property_is_output(property_name.as_str()) {
            return Err(DslParseError::unexpected_with_span(
                Rule::named_agent_value_property,
                "agent output property",
                property_span,
            ));
        }

        if agent.property_is_uses(property_name.as_str()) {
            return Ok(AgentProperty::Uses(self.visit_tools_expression(value_pair)?));
        }

        Err(DslParseError::unexpected_with_span(
            Rule::named_agent_value_property,
            "agent value property",
            property_span,
        ))
    }

    pub(super) fn visit_agent_context_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let value_pair = self.first_inner_pair(property_pair, "agent context property")?;

        Ok(AgentProperty::Context(self.visit_agent_context_value(value_pair)?))
    }

    pub(super) fn visit_agent_context_value(&self, context_value_pair: Pair<'_, Rule>) -> Result<AgentContext, DslParseError> {
        match context_value_pair.as_rule() {
            Rule::agent_context_value => {
                let inner_pair = self.first_inner_pair(context_value_pair, "agent context value")?;

                self.visit_agent_context_value(inner_pair)
            }
            Rule::reference => Ok(AgentContext::Direct(AgentContextReference {
                reference: self.visit_reference(context_value_pair.clone())?,
                explicit: false,
                span: source_span_from_pair(&context_value_pair),
            })),
            Rule::explicit_agent_context => {
                let span = source_span_from_pair(&context_value_pair);
                let reference_pair = self.first_inner_pair(context_value_pair, "explicit agent context")?;

                Ok(AgentContext::Direct(AgentContextReference {
                    reference: self.visit_reference(reference_pair)?,
                    explicit: true,
                    span,
                }))
            }
            Rule::compact_agent_context => {
                let span = source_span_from_pair(&context_value_pair);
                let mut inner_pairs = context_value_pair.into_inner();
                let reference_pair = self.next_pair(&mut inner_pairs, "compact context agent reference", "compact agent context")?;
                let reference = self.visit_reference(reference_pair)?;
                let properties = if let Some(block_pair) = inner_pairs.next() {
                    self.visit_agent_context_block(block_pair)?
                } else {
                    Vec::new()
                };

                Ok(AgentContext::Compact(CompactAgentContext {
                    reference,
                    properties,
                    span,
                }))
            }
            _ => Err(DslParseError::unexpected_with_span(
                context_value_pair.as_rule(),
                "agent context value",
                source_span_from_pair(&context_value_pair),
            )),
        }
    }

    pub(super) fn visit_agent_context_block(
        &self,
        context_block_pair: Pair<'_, Rule>,
    ) -> Result<Vec<crate::dsl::ast::ObjectField>, DslParseError> {
        let mut properties = Vec::new();

        for property_pair in context_block_pair.into_inner() {
            properties.push(self.visit_object_field(property_pair)?);
        }

        Ok(properties)
    }

    pub(super) fn visit_agent_output_property(&self, property_pair: Pair<'_, Rule>) -> Result<AgentProperty, DslParseError> {
        let span = source_span_from_pair(&property_pair);
        let mut inner_pairs = property_pair.into_inner();
        let typed_block_pair = self.next_pair(&mut inner_pairs, "agent output body", "agent output property")?;
        let fields = self.visit_typed_block(typed_block_pair)?;

        Ok(AgentProperty::Output { fields, span })
    }

    pub(super) fn visit_tools_expression(&self, tools_expression_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let mut tool_bindings = Vec::new();

        for agent_tool_binding_pair in tools_expression_pair.into_inner() {
            tool_bindings.push(self.visit_agent_tool_binding(agent_tool_binding_pair)?);
        }

        Ok(Expression::ArrayLiteral(tool_bindings))
    }

    pub(super) fn visit_agent_tool_binding(&self, agent_tool_binding_pair: Pair<'_, Rule>) -> Result<Expression, DslParseError> {
        let agent_tool_binding_span = source_span_from_pair(&agent_tool_binding_pair);
        let mut inner_pairs = agent_tool_binding_pair.into_inner();
        let callee_pair = self.next_pair(&mut inner_pairs, "agent tool binding callee", "agent tool binding")?;
        let callee = self.visit_reference(callee_pair)?;

        let Some(block_pair) = inner_pairs.next() else {
            return Ok(Expression::Reference(callee));
        };

        let block_span = source_span_from_pair(&block_pair);
        let mut binding_fields = Vec::new();
        let mut max_calls = None;

        for property_pair in block_pair.into_inner() {
            match property_pair.as_rule() {
                Rule::named_object_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "agent tool binding property name", "agent tool binding")?;
                    let Some(ToolCallPropertyName::Bindings) = ToolCallPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_object_property,
                            "agent tool binding property",
                            block_span,
                        ));
                    };
                    let object_expression_pair = self.next_pair(&mut inner_pairs, "agent tool binding bindings", "agent tool binding")?;
                    binding_fields.extend(self.visit_object_expression(object_expression_pair)?);
                }
                Rule::named_unsigned_integer_property => {
                    let mut inner_pairs = property_pair.into_inner();
                    let property_name = self.next_identifier(&mut inner_pairs, "agent tool binding property name", "agent tool binding")?;
                    let Some(ToolCallPropertyName::MaxCalls) = ToolCallPropertyName::from_identifier(property_name.as_str()) else {
                        return Err(DslParseError::unexpected_with_span(
                            Rule::named_unsigned_integer_property,
                            "agent tool binding property",
                            block_span,
                        ));
                    };
                    let max_calls_pair = self.next_pair(&mut inner_pairs, "agent tool binding max calls", "agent tool binding")?;
                    max_calls = Some(self.parse_unsigned_integer(max_calls_pair, "agent tool binding max calls property")?);
                }
                _ => {
                    return Err(DslParseError::unexpected_with_span(
                        property_pair.as_rule(),
                        "agent tool binding property",
                        source_span_from_pair(&property_pair),
                    ));
                }
            }
        }

        Ok(Expression::ToolCall(ToolCall {
            callee,
            input_fields: Vec::new(),
            binding_fields,
            max_calls,
            span: agent_tool_binding_span,
        }))
    }
}
