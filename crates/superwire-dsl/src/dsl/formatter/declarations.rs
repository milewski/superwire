use crate::dsl::ast::{
    AgentContext, AgentDeclaration, AgentForLoopPattern, AgentProperty, Declaration, DeclarationKeyword, DynamicBlock, Expression,
    ExpressionKeyword, ForClauseKeyword, ModelUsage, TypedField, Workflow,
};
use crate::dsl::structure::{self, DslProperty};

use super::expressions::{ExpressionExpressionsExt, ExpressionFormat, ObjectFieldExpressionsExt, ReferenceExpressionsExt};
use super::mcp::{
    McpBatchImportDeclarationMcpExt, McpPromptBatchImportDeclarationMcpExt, McpPromptImportDeclarationMcpExt,
    McpResourceBatchImportDeclarationMcpExt, McpResourceImportDeclarationMcpExt, McpToolBatchImportDeclarationMcpExt,
};
use super::tools::{ToolCallToolsExt, ToolDeclarationToolsExt};
use super::types::TypedFieldTypesExt;
use super::DslFormatter;

impl DslFormatter {
    pub(super) fn push_workflow(&mut self, workflow: &Workflow) {
        let mut declaration_iterator = workflow.declarations.iter().peekable();

        while let Some(declaration) = declaration_iterator.next() {
            declaration.push_to_formatter(self);

            if declaration_iterator.peek().is_some() {
                self.push_newline();
            }
        }
    }
    fn push_agent_property_expression(&mut self, property_name: &str, expression: &Expression) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(": ");
        expression.push_to_formatter(self, ExpressionFormat::Canonical);
        self.push_newline();
    }

    fn push_agent_property_type_block(&mut self, property_name: &str, fields: &[TypedField]) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(" {");
        self.push_newline();
        self.indentation_depth += 1;

        for field in fields {
            field.push_to_formatter(self);
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push('}');
        self.push_newline();
    }
}

trait DeclarationDeclarationsExt {
    fn push_to_formatter(&self, formatter: &mut DslFormatter);
}

impl DeclarationDeclarationsExt for Declaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Provider(provider_declaration) => {
                formatter.push_declaration_block_start(&format!(
                    "{} {} from {}",
                    DeclarationKeyword::Provider.as_str(),
                    provider_declaration.name,
                    provider_declaration.driver_name
                ));

                for object_field in &provider_declaration.properties {
                    object_field.push_config_property_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Model(model_declaration) => {
                formatter.push_declaration_block_start(&format!(
                    "{} {} from {}",
                    DeclarationKeyword::Model.as_str(),
                    model_declaration.name,
                    model_declaration.provider_name
                ));

                for object_field in &model_declaration.properties {
                    object_field.push_config_property_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::McpServer(mcp_server_declaration) => {
                formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Mcp.as_str(), mcp_server_declaration.name));

                for object_field in &mcp_server_declaration.properties {
                    object_field.push_config_property_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Secrets(secrets_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Secrets.as_str());

                for typed_field in &secrets_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Input(input_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Input.as_str());

                for typed_field in &input_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Schema(schema_declaration) => {
                formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Schema.as_str(), schema_declaration.name));

                for typed_field in &schema_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Tool(tool_declaration) => tool_declaration.push_to_formatter(formatter),
            Self::McpBatch(batch_import_declaration) => batch_import_declaration.push_to_formatter(formatter),
            Self::McpToolBatch(tool_batch_import_declaration) => tool_batch_import_declaration.push_to_formatter(formatter),
            Self::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration.push_to_formatter(formatter),
            Self::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration.push_to_formatter(formatter),
            Self::McpResource(resource_import_declaration) => resource_import_declaration.push_to_formatter(formatter),
            Self::McpPrompt(prompt_import_declaration) => prompt_import_declaration.push_to_formatter(formatter),
            Self::Dynamic(dynamic_block) => dynamic_block.push_to_formatter(formatter),
            Self::Agent(agent_declaration) => {
                agent_declaration.push_to_formatter(formatter);
            }
            Self::Output(output_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Output.as_str());

                for object_field in &output_declaration.fields {
                    object_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
        }
    }
}

trait AgentDeclarationDeclarationsExt {
    fn push_to_formatter(&self, formatter: &mut DslFormatter);
}

impl AgentDeclarationDeclarationsExt for AgentDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let mut declaration_header = format!("{} {}", DeclarationKeyword::Agent.as_str(), self.name);

        if let Some(loop_declaration) = &self.for_loop {
            declaration_header.push(' ');
            declaration_header.push_str(ForClauseKeyword::For.as_str());
            declaration_header.push(' ');
            declaration_header.push_str(&loop_declaration.pattern.render_for_clause());
            declaration_header.push(' ');
            declaration_header.push_str(ForClauseKeyword::In.as_str());
            declaration_header.push(' ');
            declaration_header.push_str(&formatter.inline_expression(&loop_declaration.iterable));
        }

        formatter.push_declaration_block_start(&declaration_header);

        let rendered_properties = self
            .properties
            .iter()
            .map(|agent_property| agent_property.render_for_agent_block(formatter.indentation_depth))
            .collect::<Vec<_>>();

        for (property_index, rendered_property) in rendered_properties.iter().enumerate() {
            if property_index > 0 {
                let previous_property_is_multiline = rendered_properties[property_index.saturating_sub(1)].is_multiline;
                let current_property_is_multiline = rendered_property.is_multiline;

                if previous_property_is_multiline || (current_property_is_multiline && !rendered_property.is_output_property) {
                    formatter.push_newline();
                }
            }

            formatter.output.push_str(&rendered_property.text);
        }

        formatter.push_declaration_block_end();
    }
}

trait AgentForLoopPatternDeclarationsExt {
    fn render_for_clause(&self) -> String;
}

impl AgentForLoopPatternDeclarationsExt for AgentForLoopPattern {
    fn render_for_clause(&self) -> String {
        match self {
            Self::Identifier(identifier) => identifier.clone(),
            Self::ObjectDestructuring(field_names) => format!("{{ {} }}", field_names.join(", ")),
        }
    }
}

trait AgentPropertyDeclarationsExt {
    fn push_to_formatter(&self, formatter: &mut DslFormatter);

    fn push_agent_binding_list_property(&self, formatter: &mut DslFormatter, property_name: &str, expression: &Expression);

    fn inline_agent_tool_bindings(&self, formatter: &DslFormatter, tool_bindings: &[Expression]) -> Option<String>;

    fn render_for_agent_block(&self, indentation_depth: usize) -> RenderedAgentProperty;
}

impl AgentPropertyDeclarationsExt for AgentProperty {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Dynamic(dynamic_block) => dynamic_block.push_to_formatter(formatter),
            Self::Model(model_usage) => model_usage.push_to_formatter(formatter),
            Self::InvalidModel(expression) => {
                formatter.push_agent_property_expression(structure::Agent::new().model.definition().name, expression);
            }
            Self::Instruction(expression) => {
                formatter.push_agent_property_expression(structure::Agent::new().instruction.definition().name, expression);
            }
            Self::Output { fields, span: _ } => {
                let agent = structure::Agent::new();

                formatter.push_agent_property_type_block(
                    agent.output.expect("agent structure should include output").definition().name,
                    fields,
                );
            }
            Self::Context(agent_context) => {
                let agent = structure::Agent::new();

                formatter.push_agent_context_property(
                    agent.context.expect("agent structure should include context").definition().name,
                    agent_context,
                );
            }
            Self::Uses(expression) => {
                self.push_agent_binding_list_property(formatter, structure::Agent::new().uses[0].definition().name, expression);
            }
            Self::Unknown { name: _, span: _ } => {}
        }
    }

    fn push_agent_binding_list_property(&self, formatter: &mut DslFormatter, property_name: &str, expression: &Expression) {
        let Expression::ArrayLiteral(tool_bindings) = expression else {
            formatter.push_agent_property_expression(property_name, expression);

            return;
        };

        formatter.push_indent();
        formatter.output.push_str(property_name);
        formatter.output.push_str(": ");

        if tool_bindings.is_empty() {
            formatter.output.push_str("[]");
            formatter.push_newline();

            return;
        }

        if let Some(inline_tool_bindings) = self.inline_agent_tool_bindings(formatter, tool_bindings) {
            if formatter.can_fit_inline_text(&inline_tool_bindings) {
                formatter.output.push_str(&inline_tool_bindings);
                formatter.push_newline();

                return;
            }
        }

        formatter.output.push('[');
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for tool_binding in tool_bindings {
            formatter.push_indent();
            tool_binding.push_agent_tool_binding_to_formatter(formatter);
            formatter.output.push(',');
            formatter.push_newline();
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push(']');
        formatter.push_newline();
    }

    fn inline_agent_tool_bindings(&self, formatter: &DslFormatter, tool_bindings: &[Expression]) -> Option<String> {
        let mut inline_tool_bindings = Vec::new();

        for tool_binding in tool_bindings {
            match tool_binding {
                Expression::Reference(reference) => {
                    let mut reference_formatter = DslFormatter::new();
                    reference.push_to_formatter(&mut reference_formatter);
                    inline_tool_bindings.push(reference_formatter.output);
                }
                Expression::ToolCall(tool_call) if tool_call.binding_fields.is_empty() => {
                    let mut tool_call_formatter = DslFormatter::new();
                    tool_call.push_agent_binding_to_formatter(&mut tool_call_formatter);
                    inline_tool_bindings.push(tool_call_formatter.output);
                }
                _ => return None,
            }
        }

        let inline_expression = format!("[{}]", inline_tool_bindings.join(", "));

        if formatter.can_fit_inline_text(&inline_expression) {
            Some(inline_expression)
        } else {
            None
        }
    }

    fn render_for_agent_block(&self, indentation_depth: usize) -> RenderedAgentProperty {
        let mut property_formatter = DslFormatter {
            output: String::new(),
            indentation_depth,
        };

        self.push_to_formatter(&mut property_formatter);

        let property_body_without_trailing_newline = property_formatter
            .output
            .strip_suffix('\n')
            .unwrap_or(property_formatter.output.as_str());
        let property_is_multiline = property_body_without_trailing_newline.contains('\n');

        RenderedAgentProperty {
            text: property_formatter.output,
            is_multiline: property_is_multiline,
            is_output_property: matches!(self, Self::Output { fields: _, span: _ }),
        }
    }
}

impl DslFormatter {
    fn push_agent_context_property(&mut self, property_name: &str, agent_context: &AgentContext) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(": ");

        match agent_context {
            AgentContext::Direct(agent_context_reference) => {
                if agent_context_reference.explicit {
                    self.output.push_str(ExpressionKeyword::Context.as_str());
                    self.output.push(' ');
                }

                agent_context_reference.reference.push_to_formatter(self);
                self.push_newline();
            }
            AgentContext::Compact(compact_agent_context) => {
                self.output.push_str(ExpressionKeyword::Compact.as_str());
                self.output.push(' ');
                compact_agent_context.reference.push_to_formatter(self);

                if compact_agent_context.properties.is_empty() {
                    self.push_newline();
                    return;
                }

                self.output.push_str(" {");
                self.push_newline();
                self.indentation_depth += 1;

                for property in &compact_agent_context.properties {
                    property.push_to_formatter(self);
                }

                self.indentation_depth -= 1;
                self.push_indent();
                self.output.push('}');
                self.push_newline();
            }
        }
    }
}

trait DynamicBlockDeclarationsExt {
    fn push_to_formatter(&self, formatter: &mut DslFormatter);
}

impl DynamicBlockDeclarationsExt for DynamicBlock {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(structure::Agent::new().dynamic[0].definition().name);
        formatter.output.push(' ');
        formatter.output.push('{');
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for field in &self.fields {
            field.push_to_formatter(formatter);
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
        formatter.push_newline();
    }
}

trait ModelUsageDeclarationsExt {
    fn push_to_formatter(&self, formatter: &mut DslFormatter);
}

impl ModelUsageDeclarationsExt for ModelUsage {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(structure::Agent::new().model.definition().name);
        formatter.output.push_str(": ");
        self.reference.push_to_formatter(formatter);

        if self.properties.is_empty() {
            formatter.push_newline();

            return;
        }

        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for property in &self.properties {
            property.push_config_property_to_formatter(formatter);
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
        formatter.push_newline();
    }
}

struct RenderedAgentProperty {
    text: String,
    is_multiline: bool,
    is_output_property: bool,
}
