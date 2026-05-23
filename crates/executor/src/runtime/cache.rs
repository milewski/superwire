use crate::runtime::ExecutorError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_AGENT_CACHE_TIME_TO_LIVE: Duration = Duration::from_secs(60 * 60);
pub const LOCAL_AGENT_CACHE_SESSION: &str = "local";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentCacheDriver {
    #[default]
    InMemory,
    Redis,
}

impl AgentCacheDriver {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InMemory => "in-memory",
            Self::Redis => "redis",
        }
    }

    pub fn build_store(self) -> Result<Arc<dyn AgentCacheStore>, ExecutorError> {
        match self {
            Self::InMemory => Ok(Arc::new(InMemoryAgentCacheStore::default())),
            Self::Redis => Err(ExecutorError::Other {
                message: "redis cache driver is not implemented yet".to_string(),
            }),
        }
    }
}

impl FromStr for AgentCacheDriver {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "in-memory" => Ok(Self::InMemory),
            "redis" => Ok(Self::Redis),
            _ => Err(format!("unknown cache driver `{value}`; expected in-memory or redis")),
        }
    }
}

impl fmt::Display for AgentCacheDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentCacheTimeToLive(pub Duration);

impl Default for AgentCacheTimeToLive {
    fn default() -> Self {
        Self(DEFAULT_AGENT_CACHE_TIME_TO_LIVE)
    }
}

impl FromStr for AgentCacheTimeToLive {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        humantime::parse_duration(value)
            .or_else(|_| humantime::parse_duration(&Self::spaced_duration(value)))
            .map(Self)
            .map_err(|error| format!("invalid cache ttl `{value}`: {error}"))
    }
}

impl AgentCacheTimeToLive {
    #[must_use]
    fn spaced_duration(value: &str) -> String {
        let mut spaced_value = String::new();
        let mut previous_character_was_digit = false;

        for character in value.chars() {
            let current_character_is_digit = character.is_ascii_digit();

            if previous_character_was_digit && character.is_ascii_alphabetic() {
                spaced_value.push(' ');
            }

            if !previous_character_was_digit && current_character_is_digit && !spaced_value.is_empty() {
                spaced_value.push(' ');
            }

            spaced_value.push(character);
            previous_character_was_digit = current_character_is_digit;
        }

        spaced_value
    }
}

impl fmt::Display for AgentCacheTimeToLive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", humantime::format_duration(self.0))
    }
}

#[derive(Debug, Clone)]
pub struct AgentCacheOptions {
    pub enabled: bool,
    pub session: AgentCacheSession,
    pub store: Option<Arc<dyn AgentCacheStore>>,
    pub time_to_live: Duration,
}

impl AgentCacheOptions {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            session: AgentCacheSession::local(),
            store: None,
            time_to_live: DEFAULT_AGENT_CACHE_TIME_TO_LIVE,
        }
    }

    #[must_use]
    pub fn enabled(session: AgentCacheSession, store: Arc<dyn AgentCacheStore>, time_to_live: Duration) -> Self {
        Self {
            enabled: true,
            session,
            store: Some(store),
            time_to_live,
        }
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.store.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentCacheSession {
    identifier: String,
}

impl AgentCacheSession {
    #[must_use]
    pub fn new(identifier: impl Into<String>) -> Self {
        let identifier = identifier.into();
        let identifier = identifier.trim();

        if identifier.is_empty() {
            return Self::local();
        }

        Self {
            identifier: identifier.to_string(),
        }
    }

    #[must_use]
    pub fn local() -> Self {
        Self {
            identifier: LOCAL_AGENT_CACHE_SESSION.to_string(),
        }
    }

    #[must_use]
    pub fn from_fingerprint_parts(parts: &[&str]) -> Self {
        let mut hasher = Sha256::new();

        for part in parts {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }

        let fingerprint_hash = hasher.finalize();

        Self {
            identifier: format!("fingerprint:{}", hex_digest(&fingerprint_hash)),
        }
    }

    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentCacheKey {
    session_identifier: String,
    agent_hash: String,
}

impl AgentCacheKey {
    #[must_use]
    pub fn new(session: &AgentCacheSession, agent_hash: String) -> Self {
        Self {
            session_identifier: session.identifier().to_string(),
            agent_hash,
        }
    }

    #[must_use]
    pub fn session_identifier(&self) -> &str {
        &self.session_identifier
    }

    #[must_use]
    pub fn agent_hash(&self) -> &str {
        &self.agent_hash
    }
}

#[derive(Debug, Clone)]
pub struct CachedAgentExecution {
    pub output: Value,
    pub context: Value,
}

impl CachedAgentExecution {
    #[must_use]
    pub fn new(output: Value, context: Value) -> Self {
        Self { output, context }
    }
}

pub trait AgentCacheStore: Send + Sync + fmt::Debug {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError>;
    fn put(&self, key: AgentCacheKey, execution: CachedAgentExecution, time_to_live: Duration) -> Result<(), ExecutorError>;
    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError>;
}

#[derive(Debug, Default)]
pub struct InMemoryAgentCacheStore {
    entries: Mutex<HashMap<AgentCacheKey, InMemoryAgentCacheEntry>>,
}

#[derive(Debug, Clone)]
struct InMemoryAgentCacheEntry {
    execution: CachedAgentExecution,
    expires_at: Instant,
}

impl InMemoryAgentCacheEntry {
    #[must_use]
    fn new(execution: CachedAgentExecution, time_to_live: Duration) -> Self {
        Self {
            execution,
            expires_at: Instant::now() + time_to_live,
        }
    }

    #[must_use]
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

impl AgentCacheStore for InMemoryAgentCacheStore {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError> {
        let mut entries = self.entries.lock().map_err(|error| ExecutorError::Other {
            message: format!("failed to acquire agent cache lock: {error}"),
        })?;

        let Some(entry) = entries.get(key) else {
            return Ok(None);
        };

        if entry.is_expired() {
            entries.remove(key);

            return Ok(None);
        }

        Ok(Some(entry.execution.clone()))
    }

    fn put(&self, key: AgentCacheKey, execution: CachedAgentExecution, time_to_live: Duration) -> Result<(), ExecutorError> {
        if time_to_live.is_zero() {
            return Ok(());
        }

        self.entries
            .lock()
            .map_err(|error| ExecutorError::Other {
                message: format!("failed to acquire agent cache lock: {error}"),
            })?
            .insert(key, InMemoryAgentCacheEntry::new(execution, time_to_live));

        Ok(())
    }

    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError> {
        let mut entries = self.entries.lock().map_err(|error| ExecutorError::Other {
            message: format!("failed to acquire agent cache lock: {error}"),
        })?;
        let previous_len = entries.len();

        entries.retain(|key, _| key.session_identifier() != session.identifier());

        Ok(previous_len.saturating_sub(entries.len()))
    }
}

pub fn hash_serializable_value(value: &impl serde::Serialize) -> Result<String, ExecutorError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ExecutorError::Other {
        message: format!("failed to serialize agent cache fingerprint: {error}"),
    })?;
    let mut hasher = Sha256::new();

    hasher.update(bytes);

    let hash = hasher.finalize();

    Ok(hex_digest(&hash))
}

#[must_use]
fn hex_digest(bytes: &[u8]) -> String {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

    let mut digest = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        digest.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        digest.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }

    digest
}
