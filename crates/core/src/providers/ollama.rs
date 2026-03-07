use crate::providers::{Provider, Message, ToolDefinition, Response};
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use ollama_rs::Ollama;
use log::{info, debug};

pub struct OllamaProvider {
    name: String,
    api_endpoint: String,
    models: Vec<String>,
    client: Ollama,
}

impl OllamaProvider {
    pub fn new(name: String, api_endpoint: String, models: Vec<String>) -> Self {
        // Use the API endpoint directly
        let client = Ollama::from_url(
            url::Url::parse(&api_endpoint)
                .unwrap_or_else(|_| url::Url::parse("http://localhost:11434").unwrap())
        );

        Self {
            name,
            api_endpoint,
            models,
            client,
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn driver(&self) -> &str {
        "ollama"
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    async fn execute(
        &self,
        model: &str,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<Response> {
        if !self.models.contains(&model.to_string()) {
            return Err(anyhow!("Model {} not available in provider {}", model, self.name));
        }

        info!("OllamaProvider: Executing with {} tools", tools.len());
        for tool in &tools {
            debug!("  Tool: {} - {}", tool.name, tool.description);
        }

        // Convert our messages to Ollama chat messages format
        let chat_messages: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();

        // Build the request JSON manually to include tools
        let mut request_json = serde_json::json!({
            "model": model,
            "messages": chat_messages,
            "stream": false,
        });

        // Add tools if provided
        if !tools.is_empty() {
            let tool_infos: Vec<serde_json::Value> = tools.iter().map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            }).collect();

            request_json["tools"] = serde_json::json!(tool_infos);
        }

        info!("Sending chat request to Ollama");
        debug!("Request: {}", serde_json::to_string_pretty(&request_json)?);

        // Send the request directly using reqwest
        let url = format!("{}api/chat", self.client.url_str());
        let response = reqwest::Client::new()
            .post(&url)
            .json(&request_json)
            .send()
            .await
            .map_err(|e| anyhow!("Ollama API error: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Ollama API error: {}", error_text));
        }

        let response_json: serde_json::Value = response.json().await?;
        debug!("Response: {}", serde_json::to_string_pretty(&response_json)?);

        // Extract the message content
        let content = response_json["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        info!("Received response from Ollama");
        debug!("Response content: {}", content);

        // Extract tool calls from the response
        let tool_calls: Vec<crate::providers::ToolCall> = if let Some(tool_calls_json) = response_json["message"]["tool_calls"].as_array() {
            tool_calls_json.iter().filter_map(|tc| {
                let function = &tc["function"];
                let name = function["name"].as_str()?.to_string();
                let arguments = function["arguments"].clone();

                Some(crate::providers::ToolCall {
                    name,
                    arguments,
                })
            }).collect()
        } else {
            Vec::new()
        };

        if !tool_calls.is_empty() {
            info!("Received {} tool call(s) from Ollama", tool_calls.len());
        } else {
            debug!("No tool calls in response");
        }

        Ok(Response {
            content: Some(content),
            tool_calls,
        })
    }
}
