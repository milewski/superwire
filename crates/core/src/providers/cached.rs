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
    pub agents: HashMap<String, Vec<AgentConversation>>,
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
            .join("tests")
            .join(".cache")
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
        let replay_result = {
            let cache = self.cache.lock().unwrap();
            let mut replay_indices = self.agent_replay_indices.lock().unwrap();

            let replay_index = replay_indices.entry(agent_name.clone()).or_insert(0);

            if let Some(agent_conversations) = cache.agents.get(&agent_name) {
                // Get the conversation at the current replay index
                if *replay_index < agent_conversations.len() {
                    let conversation = &agent_conversations[*replay_index];
                    *replay_index += 1;

                    Some(conversation.messages.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(replay_context) = replay_result {
            if let Some(Message::Assistant { content, .. }) = replay_context.last() {
                let output_value = serde_json::json!({ "content": content });

                return Ok(AgentOutput {
                    output: output_value,
                    context: replay_context,
                });
            }
        }

        // No cache available, call the real provider
        if let Some(ref inner) = self.inner {
            let output = inner.execute_agent(agent, context.clone(), tools).await?;

            // Save the complete conversation to cache
            {
                let mut cache = self.cache.lock().unwrap();

                let agent_conversations = cache.agents.entry(agent_name.clone()).or_default();

                // Add this execution as a new conversation
                agent_conversations.push(AgentConversation {
                    model: model.clone(),
                    messages: output.context.clone(),
                });

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
