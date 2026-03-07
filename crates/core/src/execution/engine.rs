use crate::ast::*;
use crate::providers::{Provider, Message, ToolDefinition, Response};
use crate::tools::{Tool, DoneTool};
use crate::schemas::compile_schema;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use log::{info, debug, warn, error};

pub struct ExecutionEngine {
    providers: HashMap<String, Arc<dyn Provider>>,
    tools: HashMap<String, Arc<dyn Tool>>,
    schemas: HashMap<String, Schema>,
}

impl ExecutionEngine {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert("done".to_string(), Arc::new(DoneTool));

        Self {
            providers: HashMap::new(),
            tools,
            schemas: HashMap::new(),
        }
    }

    pub fn add_provider(&mut self, provider: Arc<dyn Provider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn add_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn add_schema(&mut self, name: String, schema: Schema) {
        self.schemas.insert(name, schema);
    }

    pub async fn execute_agent(
        &self,
        agent: &Agent,
        context: &mut AgentContext,
        document_schemas: &HashMap<String, Schema>,
    ) -> Result<Value> {
        info!("=== Starting Agent Execution ===");
        info!("Agent: {}", agent.name);

        // Resolve the model and provider
        let (provider_name, model_name) = self.parse_model_ref(&agent.model)?;
        info!("Model: {}/{}", provider_name, model_name);

        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow!("Provider '{}' not found", provider_name))?;

        // Log available tools
        let mut all_tools = agent.tools.clone();
        all_tools.push("done".to_string());
        info!("Available tools: {:?}", all_tools);

        // Build initial messages
        let mut messages = Vec::new();

        // Add context if specified
        if let Some(context_ref) = &agent.context {
            debug!("Loading context from: {:?}", context_ref);
            let context_messages = context.get_context(context_ref)?;
            info!("Loaded {} context messages", context_messages.len());
            messages.extend(context_messages);
        }

        // Add the prompt
        let prompt_text = self.resolve_prompt(&agent.prompt, context)?;
        info!("Prompt: {}", prompt_text);
        messages.push(Message {
            role: "user".to_string(),
            content: prompt_text,
        });

        // Compile schema if output is specified
        let schema = if let Some(schema_ref) = &agent.output {
            info!("Output schema required: {:?}", schema_ref);
            Some(self.compile_schema_ref(schema_ref, document_schemas)?)
        } else {
            info!("No output schema specified");
            None
        };

        // Build tool definitions
        let mut tool_defs = Vec::new();
        for tool_name in &agent.tools {
            if let Some(tool) = self.tools.get(tool_name) {
                tool_defs.push(ToolDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: serde_json::json!({}),
                });
            }
        }

        // Always add the done tool with the output schema as its parameters
        let done_parameters = if let Some(schema_json) = &schema {
            // Use the output schema as the done tool's parameters
            schema_json.clone()
        } else {
            // No schema specified - accept any JSON
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
            })
        };

        tool_defs.push(ToolDefinition {
            name: "done".to_string(),
            description: "Call this tool with your final output to exit the agent loop. Pass the result directly as the tool arguments.".to_string(),
            parameters: done_parameters,
        });

        info!("Tool definitions being sent to provider:");
        for tool_def in &tool_defs {
            info!("  - {} : {}", tool_def.name, tool_def.description);
        }

        // Track messages for this agent's context
        let mut agent_messages = messages.clone();

        // Agent execution loop
        let max_iterations = 50;
        info!("Starting agent loop (max {} iterations)", max_iterations);

        for iteration in 0..max_iterations {
            info!("--- Iteration {} ---", iteration + 1);

            // Call the provider
            debug!("Sending request to provider with {} messages", messages.len());
            let response = provider.execute(model_name, messages.clone(), tool_defs.clone()).await?;

            // Store the assistant's response in context
            if let Some(content) = &response.content {
                info!("Agent response: {}", content);
                context.add_message(Message {
                    role: "assistant".to_string(),
                    content: content.clone(),
                });
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: content.clone(),
                });
                agent_messages.push(Message {
                    role: "assistant".to_string(),
                    content: content.clone(),
                });
            } else {
                debug!("No content in response");
            }

            // Check if there are tool calls
            if response.tool_calls.is_empty() {
                warn!("No tool calls in response (iteration {}/{})", iteration + 1, max_iterations);

                // No tool calls, continue the loop
                if iteration == max_iterations - 1 {
                    error!("Agent exceeded maximum iterations without calling 'done'");
                    return Err(anyhow!("Agent exceeded maximum iterations without calling 'done'"));
                }
                continue;
            }

            info!("Received {} tool call(s)", response.tool_calls.len());

            // Process tool calls
            for tool_call in &response.tool_calls {
                info!("Tool call: {} with args: {}", tool_call.name, tool_call.arguments);

                if tool_call.name == "done" {
                    info!("Agent called 'done' tool - completing execution");

                    // Agent is done, extract and validate output
                    let mut output = tool_call.arguments.clone();

                    // If the output is empty or just an empty object, use the content instead
                    if output.is_null() || (output.is_object() && output.as_object().unwrap().is_empty()) {
                        if let Some(content) = &response.content {
                            if !content.is_empty() {
                                info!("Using response content as output (tool arguments were empty)");
                                output = serde_json::json!({"result": content});
                            }
                        }
                    }

                    debug!("Output: {}", serde_json::to_string_pretty(&output).unwrap_or_default());

                    // Validate against schema if specified
                    if let Some(schema_json) = &schema {
                        info!("Validating output against schema...");
                        match crate::schemas::validate_against_schema(&output, schema_json) {
                            Ok(_) => {
                                info!("✓ Schema validation passed");
                            }
                            Err(e) => {
                                error!("✗ Schema validation failed: {}", e);
                                return Err(e);
                            }
                        }
                    }

                    info!("=== Agent Execution Complete ===");
                    return Ok(output);
                } else {
                    // Execute other tools
                    if let Some(tool) = self.tools.get(&tool_call.name) {
                        debug!("Executing tool: {}", tool_call.name);
                        let result = tool.execute(tool_call.arguments.clone())?;
                        info!("Tool '{}' result: {}", tool_call.name, result);

                        // Add tool result to messages
                        let tool_message = Message {
                            role: "tool".to_string(),
                            content: format!("Tool '{}' result: {}", tool_call.name, result),
                        };
                        messages.push(tool_message.clone());
                        agent_messages.push(tool_message);
                    } else {
                        error!("Tool '{}' not found", tool_call.name);
                        return Err(anyhow!("Tool '{}' not found", tool_call.name));
                    }
                }
            }
        }

        error!("Agent exceeded maximum iterations");
        Err(anyhow!("Agent exceeded maximum iterations"))
    }

    fn parse_model_ref<'a>(&self, model_ref: &'a Option<String>) -> Result<(&'a str, &'a str)> {
        let model_ref = model_ref.as_ref()
            .ok_or_else(|| anyhow!("Agent has no model specified"))?;

        let parts: Vec<&str> = model_ref.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid model reference format: '{}'", model_ref));
        }

        Ok((parts[0], parts[1]))
    }

    fn resolve_prompt(&self, prompt: &PromptValue, context: &AgentContext) -> Result<String> {
        match prompt {
            PromptValue::Inline(s) | PromptValue::Multiline(s) => {
                Ok(self.interpolate_string(s, context)?)
            }
            PromptValue::Function(func) => {
                self.resolve_function_call(func, context)
            }
        }
    }

    fn resolve_function_call(&self, func: &FunctionCall, context: &AgentContext) -> Result<String> {
        match func.name.as_str() {
            "file" => {
                // Extract the file path from the first argument (it's embedded in the function structure)
                // The path is the string argument to the function itself
                // For now, we'll need to extract it from the args
                let mut replacements = std::collections::HashMap::new();

                for (arg_name, arg_value) in &func.args {
                    let value = match arg_value {
                        FunctionArg::String(s) => self.interpolate_string(s, context)?,
                        FunctionArg::Function(nested) => self.resolve_function_call(nested, context)?,
                    };
                    replacements.insert(arg_name.clone(), value);
                }

                // The file path should be extracted from the function call structure
                // For now, return an error indicating this needs the path
                Err(anyhow!("File function requires path parameter - not yet fully implemented"))
            }
            _ => Err(anyhow!("Unknown function: {}", func.name))
        }
    }

    fn interpolate_string(&self, s: &str, context: &AgentContext) -> Result<String> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                if let Some(&'{') = chars.peek() {
                    chars.next(); // consume second {
                    let mut var_name = String::new();

                    while let Some(c) = chars.next() {
                        if c == '}' {
                            if let Some(&'}') = chars.peek() {
                                chars.next(); // consume second }

                                // Resolve the variable
                                let value = context.get_variable(var_name.trim())?;
                                result.push_str(&value);
                                break;
                            }
                        }
                        var_name.push(c);
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        Ok(result)
    }

    fn compile_schema_ref(&self, schema_ref: &SchemaRef, document_schemas: &HashMap<String, Schema>) -> Result<Value> {
        match schema_ref {
            SchemaRef::Named(name) => {
                // Look up schema from the document
                let schema = document_schemas.get(name)
                    .ok_or_else(|| anyhow!("Schema '{}' not found in document", name))?;
                compile_schema(schema)
            }
            SchemaRef::Inline(schema) => {
                compile_schema(schema)
            }
        }
    }
}

pub struct AgentContext {
    messages: Vec<Message>,
    variables: HashMap<String, Value>,
    agent_contexts: HashMap<String, Vec<Message>>,
}

impl Clone for AgentContext {
    fn clone(&self) -> Self {
        Self {
            messages: self.messages.clone(),
            variables: self.variables.clone(),
            agent_contexts: self.agent_contexts.clone(),
        }
    }
}

impl AgentContext {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            variables: HashMap::new(),
            agent_contexts: HashMap::new(),
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn get_context(&self, context_ref: &ContextRef) -> Result<Vec<Message>> {
        match context_ref {
            ContextRef::Full(agent_name) => {
                // Return the full context from the referenced agent
                if let Some(context) = self.agent_contexts.get(agent_name) {
                    Ok(context.clone())
                } else {
                    Ok(Vec::new())
                }
            }
            ContextRef::Summary(agent_name) => {
                // Return a summarized version of the context
                if let Some(context) = self.agent_contexts.get(agent_name) {
                    // For now, just return the last few messages as a summary
                    // In a production system, this would use an LLM to generate a summary
                    let summary_length = 3.min(context.len());
                    let summary_messages = context[context.len().saturating_sub(summary_length)..].to_vec();
                    Ok(summary_messages)
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    pub fn save_agent_context(&mut self, agent_name: String, messages: Vec<Message>) {
        self.agent_contexts.insert(agent_name, messages);
    }

    pub fn set_variable(&mut self, name: String, value: Value) {
        self.variables.insert(name, value);
    }

    pub fn get_variable(&self, name: &str) -> Result<String> {
        // Parse variable reference like "agent.field"
        let parts: Vec<&str> = name.split('.').collect();

        if parts.len() == 2 {
            let agent_name = parts[0];
            let field_name = parts[1];

            if let Some(value) = self.variables.get(agent_name) {
                if let Some(field_value) = value.get(field_name) {
                    return Ok(field_value.to_string().trim_matches('"').to_string());
                }
            }
        }

        Err(anyhow!("Variable '{}' not found", name))
    }

    pub fn get_output(&self, agent_name: &str) -> Option<&Value> {
        self.variables.get(agent_name)
    }
}

