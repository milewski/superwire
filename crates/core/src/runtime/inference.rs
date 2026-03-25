use crate::runtime::error::WorkflowRuntimeError;
use engine_ai_agent::AgentConfig;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceSetting {
    MaxTokens,
    Temperature,
    TopP,
    TopK,
    FrequencyPenalty,
    PresencePenalty,
    RepeatPenalty,
    Seed,
    StuckThreshold,
    ProviderMaxRetries,
    ProviderRetryBaseDelayMs,
}

impl InferenceSetting {
    #[must_use]
    pub fn all() -> [Self; 11] {
        [
            Self::MaxTokens,
            Self::Temperature,
            Self::TopP,
            Self::TopK,
            Self::FrequencyPenalty,
            Self::PresencePenalty,
            Self::RepeatPenalty,
            Self::Seed,
            Self::StuckThreshold,
            Self::ProviderMaxRetries,
            Self::ProviderRetryBaseDelayMs,
        ]
    }

    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::MaxTokens => "max_tokens",
            Self::Temperature => "temperature",
            Self::TopP => "top_p",
            Self::TopK => "top_k",
            Self::FrequencyPenalty => "frequency_penalty",
            Self::PresencePenalty => "presence_penalty",
            Self::RepeatPenalty => "repeat_penalty",
            Self::Seed => "seed",
            Self::StuckThreshold => "stuck_threshold",
            Self::ProviderMaxRetries => "provider_max_retries",
            Self::ProviderRetryBaseDelayMs => "provider_retry_base_delay_ms",
        }
    }

    pub fn apply(
        self,
        config: AgentConfig,
        inference_fields: &Map<String, Value>,
        agent_name: &str,
    ) -> Result<AgentConfig, WorkflowRuntimeError> {
        let Some(raw_value) = inference_fields.get(self.key()) else {
            return Ok(config);
        };

        match self {
            Self::MaxTokens => {
                let parsed_value = parse_u64(raw_value, self.key())?;
                let parsed_value = convert_u64_to_usize(parsed_value, self.key(), agent_name)?;

                Ok(config.with_max_tokens(parsed_value))
            }
            Self::Temperature => {
                let parsed_value = parse_f32(raw_value, self.key())?;

                Ok(config.with_temperature(parsed_value))
            }
            Self::TopP => {
                let parsed_value = parse_f32(raw_value, self.key())?;

                Ok(config.with_top_p(parsed_value))
            }
            Self::TopK => {
                let parsed_value = parse_u64(raw_value, self.key())?;
                let parsed_value = convert_u64_to_u32(parsed_value, self.key(), agent_name)?;

                Ok(config.with_top_k(parsed_value))
            }
            Self::FrequencyPenalty => {
                let parsed_value = parse_f32(raw_value, self.key())?;

                Ok(config.with_frequency_penalty(parsed_value))
            }
            Self::PresencePenalty => {
                let parsed_value = parse_f32(raw_value, self.key())?;

                Ok(config.with_presence_penalty(parsed_value))
            }
            Self::RepeatPenalty => {
                let parsed_value = parse_f32(raw_value, self.key())?;

                Ok(config.with_repeat_penalty(parsed_value))
            }
            Self::Seed => {
                let parsed_value = parse_i32(raw_value, self.key(), agent_name)?;

                Ok(config.with_seed(parsed_value))
            }
            Self::StuckThreshold => {
                let parsed_value = parse_u64(raw_value, self.key())?;
                let parsed_value = convert_u64_to_usize(parsed_value, self.key(), agent_name)?;

                Ok(config.with_stuck_threshold(parsed_value))
            }
            Self::ProviderMaxRetries => {
                let parsed_value = parse_u64(raw_value, self.key())?;
                let parsed_value = convert_u64_to_usize(parsed_value, self.key(), agent_name)?;

                Ok(config.with_provider_max_retries(parsed_value))
            }
            Self::ProviderRetryBaseDelayMs => {
                let parsed_value = parse_u64(raw_value, self.key())?;

                Ok(config.with_provider_retry_base_delay_ms(parsed_value))
            }
        }
    }
}

fn parse_u64(raw_value: &Value, field_name: &str) -> Result<u64, WorkflowRuntimeError> {
    let Some(parsed_value) = raw_value.as_u64() else {
        return Err(WorkflowRuntimeError::Other {
            message: format!("inference `{field_name}` must be a non-negative integer"),
        });
    };

    Ok(parsed_value)
}

fn parse_i32(raw_value: &Value, field_name: &str, agent_name: &str) -> Result<i32, WorkflowRuntimeError> {
    let Some(parsed_value) = raw_value.as_i64() else {
        return Err(WorkflowRuntimeError::InvalidAgentProperty {
            agent_name: agent_name.to_string(),
            property: "inference".to_string(),
            message: format!("`{field_name}` must be an integer"),
        });
    };

    let parsed_value = i32::try_from(parsed_value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds i32 range"),
    })?;

    Ok(parsed_value)
}

fn parse_f32(raw_value: &Value, field_name: &str) -> Result<f32, WorkflowRuntimeError> {
    let parsed_value = serde_json::from_value::<f32>(raw_value.clone()).map_err(|_| WorkflowRuntimeError::Other {
        message: format!("inference `{field_name}` must be numeric"),
    })?;

    if !parsed_value.is_finite() {
        return Err(WorkflowRuntimeError::Other {
            message: format!("inference `{field_name}` must be numeric"),
        });
    }

    Ok(parsed_value)
}

fn convert_u64_to_usize(value: u64, field_name: &str, agent_name: &str) -> Result<usize, WorkflowRuntimeError> {
    usize::try_from(value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds usize range"),
    })
}

fn convert_u64_to_u32(value: u64, field_name: &str, agent_name: &str) -> Result<u32, WorkflowRuntimeError> {
    u32::try_from(value).map_err(|_| WorkflowRuntimeError::InvalidAgentProperty {
        agent_name: agent_name.to_string(),
        property: "inference".to_string(),
        message: format!("`{field_name}` exceeds u32 range"),
    })
}
