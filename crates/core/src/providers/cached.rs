use crate::ast::Agent;
use crate::providers::error::ProviderError;
use crate::providers::provider::{AgentOutput, Message, Provider, ToolDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCache {
    pub workflow_hash: String,
    pub agents: HashMap<String, AgentConversation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConversation {
    pub model: String,
    pub messages: Vec<Message>,
}

pub struct CachedProvider {
    name: String,
    models: Vec<String>,
    test_name: String,
    inner: Option<Arc<dyn Provider>>,
    cache: Arc<Mutex<TestCache>>,
    agent_replay_indices: Arc<Mutex<HashMap<String, usize>>>,
}

impl CachedProvider {
    pub fn new(test_name: String, workflow_hash: String, inner: Arc<dyn Provider>) -> Self {
        let cache = Self::load_cache(&test_name, &workflow_hash);

        Self {
            name: format!("cached_{}", inner.name()),
            models: inner.models().to_vec(),
            test_name,
            inner: Some(inner),
            cache: Arc::new(Mutex::new(cache)),
            agent_replay_indices: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn load_cache(test_name: &str, workflow_hash: &str) -> TestCache {
        let cache_path = Self::cache_path(test_name);

        if cache_path.exists() {
            let content = fs::read_to_string(&cache_path).unwrap_or_default();
            if let Ok(cache) = serde_json::from_str::<TestCache>(&content) {
                if cache.workflow_hash == workflow_hash {
                    return cache;
                }
            }
        }

        TestCache {
            workflow_hash: workflow_hash.to_string(),
            agents: HashMap::new(),
        }
    }

    fn save_cache(test_name: &str, cache: &TestCache) {
        let cache_path = Self::cache_path(test_name);

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let content = serde_json::to_string_pretty(cache).unwrap();
        fs::write(&cache_path, content).ok();
    }

    fn cache_path(test_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test_cache")
            .join(format!("{test_name}.json"))
    }

    fn get_model_from_agent(agent: &Agent) -> String {
        for property in &agent.properties {
            if let crate::ast::AgentProperty::Model { value, .. } = property {
                if let crate::ast::Value::String(model) = value {
                    return model.clone();
                } else if let crate::ast::Value::Interpolated(model) = value {
                    return model.clone();
                }
            }
        }
        "unknown".to_string()
    }
}

#[async_trait::async_trait]
impl Provider for CachedProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> &[String] {
        &self.models
    }

    async fn execute_agent(
        &self,
        agent: &Agent,
        context: Vec<Message>,
        tools: Vec<ToolDefinition>,
    ) -> Result<AgentOutput, ProviderError> {
        let agent_name = agent.name.clone();
        let model = Self::get_model_from_agent(agent);

        // Try to replay from cache
        let (can_replay, cached_messages) = {
            let cache = self.cache.lock().unwrap();
            let mut replay_indices = self.agent_replay_indices.lock().unwrap();

            let replay_index = replay_indices.entry(agent_name.clone()).or_insert(0);

            if let Some(agent_conv) = cache.agents.get(&agent_name) {
                if *replay_index < agent_conv.messages.len() {
                    let mut messages = Vec::new();
                    let mut found_assistant = false;

                    for message in agent_conv.messages.iter().skip(*replay_index) {
                        messages.push(message.clone());
                        *replay_index += 1;

                        if matches!(message, Message::Assistant { .. }) {
                            found_assistant = true;
                            break;
                        }
                    }

                    (found_assistant, messages)
                } else {
                    (false, Vec::new())
                }
            } else {
                (false, Vec::new())
            }
        };

        if can_replay {
            let mut new_context = context.clone();
            new_context.extend(cached_messages.clone());

            if let Some(Message::Assistant { content, .. }) = cached_messages.last() {
                let output_value = serde_json::json!({ "content": content });

                return Ok(AgentOutput {
                    output: output_value,
                    context: new_context,
                });
            }
        }

        // No cache available, call the real provider
        if let Some(ref inner) = self.inner {
            let output = inner.execute_agent(agent, context.clone(), tools).await?;

            // Save the new messages to cache
            {
                let mut cache = self.cache.lock().unwrap();

                let agent_conv = cache
                    .agents
                    .entry(agent_name.clone())
                    .or_insert_with(|| AgentConversation {
                        model: model.clone(),
                        messages: Vec::new(),
                    });

                // Add only the new messages that weren't in the original context
                let original_len = context.len();
                for message in output.context.iter().skip(original_len) {
                    agent_conv.messages.push(message.clone());
                }

                Self::save_cache(&self.test_name, &cache);
            }

            Ok(output)
        } else {
            Err(ProviderError::ExecutionError {
                message: "No cached response available and no fallback provider configured".to_string(),
            })
        }
    }
}
