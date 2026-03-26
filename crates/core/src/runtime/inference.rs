use crate::dsl::Expression;
use crate::runtime::error::WorkflowRuntimeError;
use engine_ai_agent::AgentConfig;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "max_tokens" => Some(Self::MaxTokens),
            "temperature" => Some(Self::Temperature),
            "top_p" => Some(Self::TopP),
            "top_k" => Some(Self::TopK),
            "frequency_penalty" => Some(Self::FrequencyPenalty),
            "presence_penalty" => Some(Self::PresencePenalty),
            "repeat_penalty" => Some(Self::RepeatPenalty),
            "seed" => Some(Self::Seed),
            "stuck_threshold" => Some(Self::StuckThreshold),
            "provider_max_retries" => Some(Self::ProviderMaxRetries),
            "provider_retry_base_delay_ms" => Some(Self::ProviderRetryBaseDelayMs),
            _ => None,
        }
    }

    #[must_use]
    pub fn expected_value_description(self) -> &'static str {
        match self {
            Self::MaxTokens | Self::TopK | Self::StuckThreshold | Self::ProviderMaxRetries | Self::ProviderRetryBaseDelayMs => {
                "a non-negative integer"
            }
            Self::Seed => "an integer",
            Self::Temperature | Self::TopP | Self::FrequencyPenalty | Self::PresencePenalty | Self::RepeatPenalty => "a numeric value",
        }
    }

    #[must_use]
    pub fn accepts_expression(self, expression: &Expression) -> bool {
        match expression {
            Expression::NumberLiteral(number_literal) => self.accepts_number_literal(number_literal),
            Expression::Reference(_) | Expression::FunctionCall(_) => true,
            Expression::StringLiteral(_)
            | Expression::StringTemplate(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral
            | Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_) => false,
        }
    }

    fn accepts_number_literal(self, number_literal: &str) -> bool {
        let normalized_number_literal = number_literal.replace('_', "");

        match self {
            Self::MaxTokens | Self::TopK | Self::StuckThreshold | Self::ProviderMaxRetries | Self::ProviderRetryBaseDelayMs => {
                normalized_number_literal.parse::<u64>().is_ok()
            }
            Self::Seed => normalized_number_literal.parse::<i64>().is_ok(),
            Self::Temperature | Self::TopP | Self::FrequencyPenalty | Self::PresencePenalty | Self::RepeatPenalty => {
                normalized_number_literal.parse::<f64>().is_ok()
            }
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
