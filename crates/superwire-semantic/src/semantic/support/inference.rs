use superwire_types::ast::Expression;

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
            Expression::Reference(_)
            | Expression::FunctionCall(_)
            | Expression::AgentContext(_)
            | Expression::Asset(_)
            | Expression::NullFallback(_)
            | Expression::VariantProjection(_)
            | Expression::Match(_) => true,
            Expression::StringLiteral(_)
            | Expression::StringTemplate(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral
            | Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_)
            | Expression::McpCall(_)
            | Expression::ToolCall(_) => false,
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
}
