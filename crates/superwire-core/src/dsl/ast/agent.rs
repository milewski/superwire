use super::{
    AgentExpressionPropertyName, DynamicBlock, Expression, ModelDeclaration, ModelUsagePropertyName, ObjectField, ProviderDeclaration,
    Reference, ReferenceKeyword, SourceSpan, TypeExpression, TypedField,
};
use crate::dsl::structure::{self, DslProperty, PropertyDefinition as DslPropertyDefinition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeclaration {
    pub name: String,
    pub for_loop: Option<AgentForLoop>,
    pub properties: Vec<AgentProperty>,
    pub span: SourceSpan,
}

impl AgentDeclaration {
    pub fn dynamic_blocks(&self) -> impl Iterator<Item = &DynamicBlock> {
        self.properties.iter().filter_map(|property| match property {
            AgentProperty::Dynamic(dynamic_block) => Some(dynamic_block),
            AgentProperty::Model(_)
            | AgentProperty::InvalidModel(_)
            | AgentProperty::Instruction(_)
            | AgentProperty::Output { fields: _, span: _ }
            | AgentProperty::Context(_)
            | AgentProperty::Uses(_)
            | AgentProperty::Unknown { name: _, span: _ } => None,
        })
    }

    #[must_use]
    pub fn expression_property(&self, property_name: AgentExpressionPropertyName) -> Option<&Expression> {
        for agent_property in &self.properties {
            match agent_property {
                AgentProperty::Instruction(expression) if property_name == AgentExpressionPropertyName::Instruction => {
                    return Some(expression);
                }
                AgentProperty::Context(expression) if property_name == AgentExpressionPropertyName::Context => {
                    return Some(expression);
                }
                AgentProperty::Uses(expression) if property_name == AgentExpressionPropertyName::Uses => return Some(expression),
                AgentProperty::Dynamic(_) => {}
                AgentProperty::Model(_)
                | AgentProperty::InvalidModel(_)
                | AgentProperty::Instruction(_)
                | AgentProperty::Output { fields: _, span: _ }
                | AgentProperty::Context(_)
                | AgentProperty::Uses(_)
                | AgentProperty::Unknown { name: _, span: _ } => {}
            }
        }

        None
    }

    #[must_use]
    pub fn model_usage(&self) -> Option<&ModelUsage> {
        for agent_property in &self.properties {
            if let AgentProperty::Model(model_usage) = agent_property {
                return Some(model_usage);
            }
        }

        None
    }

    #[must_use]
    pub fn effective_inference_fields(
        &self,
        _provider_declaration: Option<&ProviderDeclaration>,
        model_declaration: &ModelDeclaration,
    ) -> Vec<ObjectField> {
        let model_inference_fields = model_declaration.inference_fields().unwrap_or_default();
        let model_usage_inference_fields = self.model_usage().and_then(ModelUsage::inference_fields).unwrap_or_default();

        ObjectField::merged_with_overrides(model_inference_fields, model_usage_inference_fields)
    }

    pub fn required_expression_property(
        &self,
        property_name: AgentExpressionPropertyName,
    ) -> Result<&Expression, AgentExpressionPropertyName> {
        self.expression_property(property_name).ok_or(property_name)
    }

    #[must_use]
    pub fn output_type(&self) -> Option<TypeExpression> {
        for agent_property in &self.properties {
            if let Some(output_type_expression) = agent_property.output_type_expression() {
                return Some(output_type_expression);
            }
        }

        None
    }

    #[must_use]
    pub fn declared_final_output_type_expression(&self) -> Option<TypeExpression> {
        let output_type_expression = self.output_type()?;

        if self.for_loop.is_some() {
            return Some(TypeExpression::Array {
                item_type: Box::new(output_type_expression),
                fixed_length: None,
            });
        }

        Some(output_type_expression)
    }

    #[must_use]
    pub fn inferred_iteration_output_type_expression(&self) -> TypeExpression {
        self.output_type().unwrap_or(TypeExpression::String)
    }

    #[must_use]
    pub fn inferred_final_output_type_expression(&self) -> TypeExpression {
        let iteration_output_type_expression = self.inferred_iteration_output_type_expression();

        if self.for_loop.is_some() {
            return TypeExpression::Array {
                item_type: Box::new(iteration_output_type_expression),
                fixed_length: None,
            };
        }

        iteration_output_type_expression
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentForLoop {
    pub pattern: AgentForLoopPattern,
    pub iterable: Expression,
}

impl AgentForLoop {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        self.pattern.bound_identifier_names()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentForLoopPattern {
    Identifier(String),
    ObjectDestructuring(Vec<String>),
}

impl AgentForLoopPattern {
    #[must_use]
    pub fn bound_identifier_names(&self) -> Vec<&str> {
        match self {
            Self::Identifier(identifier) => vec![identifier.as_str()],
            Self::ObjectDestructuring(field_names) => field_names.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProperty {
    Dynamic(DynamicBlock),
    Model(ModelUsage),
    InvalidModel(Expression),
    Instruction(Expression),
    Output { fields: Vec<TypedField>, span: SourceSpan },
    Context(Expression),
    Uses(Expression),
    Unknown { name: String, span: SourceSpan },
}

impl AgentProperty {
    #[must_use]
    pub fn output_type_expression(&self) -> Option<TypeExpression> {
        match self {
            Self::Output { fields, span: _ } => Some(TypeExpression::Object(fields.clone())),
            Self::Dynamic(_)
            | Self::Model(_)
            | Self::InvalidModel(_)
            | Self::Instruction(_)
            | Self::Context(_)
            | Self::Uses(_)
            | Self::Unknown { name: _, span: _ } => None,
        }
    }

    #[must_use]
    pub fn definition(&self) -> Option<DslPropertyDefinition> {
        let agent = structure::Agent::new();

        let property_definition = match self {
            Self::Dynamic(_) => agent.dynamic[0].definition(),
            Self::Model(_) | Self::InvalidModel(_) => agent.model.definition(),
            Self::Instruction(_) => agent.instruction.definition(),
            Self::Output { fields: _, span: _ } => agent.output.expect("agent structure should include output").definition(),
            Self::Context(_) => agent.context.expect("agent structure should include context").definition(),
            Self::Uses(_) => agent.uses[0].definition(),
            Self::Unknown { name: _, span: _ } => return None,
        };

        Some(property_definition)
    }

    #[must_use]
    pub fn name(&self) -> Option<&'static str> {
        self.definition().map(|property_definition| property_definition.name)
    }

    #[must_use]
    pub fn repeatable(&self) -> bool {
        self.definition().is_some_and(|property_definition| property_definition.repeatable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub reference: Reference,
    pub properties: Vec<ObjectField>,
    pub span: SourceSpan,
}

impl ModelUsage {
    #[must_use]
    pub fn model_name(&self) -> Option<&str> {
        self.reference.direct_required_name_for_keyword(ReferenceKeyword::Model)
    }

    #[must_use]
    pub fn inference_fields(&self) -> Option<&[ObjectField]> {
        for property in &self.properties {
            if ModelUsagePropertyName::from_identifier(property.name.as_str()) != Some(ModelUsagePropertyName::Inference) {
                continue;
            }

            let Expression::ObjectLiteral(inference_fields) = &property.value else {
                return None;
            };

            return Some(inference_fields.as_slice());
        }

        None
    }
}
