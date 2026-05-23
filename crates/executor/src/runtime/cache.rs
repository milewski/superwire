use crate::runtime::ExecutorError;
use redis::Commands;
use serde::{Deserialize, Serialize};
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
        AgentCacheConfig::new(self).build_store()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCacheConfig {
    driver: AgentCacheDriver,
    redis: RedisAgentCacheConfig,
}

impl AgentCacheConfig {
    #[must_use]
    pub fn new(driver: AgentCacheDriver) -> Self {
        Self {
            driver,
            redis: RedisAgentCacheConfig::default(),
        }
    }

    #[must_use]
    pub fn with_redis(mut self, redis: RedisAgentCacheConfig) -> Self {
        self.redis = redis;

        self
    }

    pub fn build_store(&self) -> Result<Arc<dyn AgentCacheStore>, ExecutorError> {
        match self.driver {
            AgentCacheDriver::InMemory => Ok(Arc::new(InMemoryAgentCacheStore::default())),
            AgentCacheDriver::Redis => {
                RedisAgentCacheStore::new(self.redis.clone()).map(|store| Arc::new(store) as Arc<dyn AgentCacheStore>)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisAgentCacheConfig {
    host: String,
    password: Option<String>,
    database: u8,
    key_prefix: String,
}

impl Default for RedisAgentCacheConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1:6379".to_string(),
            password: None,
            database: 0,
            key_prefix: "superwire:agent-cache".to_string(),
        }
    }
}

impl RedisAgentCacheConfig {
    #[must_use]
    pub fn new(host: impl Into<String>, password: Option<String>, database: u8) -> Self {
        Self {
            host: host.into(),
            password: password.filter(|value| !value.is_empty()),
            database,
            key_prefix: Self::default().key_prefix,
        }
    }

    #[must_use]
    pub fn with_key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = key_prefix.into();

        self
    }

    #[must_use]
    fn connection_url(&self) -> String {
        let host = self.host.trim().trim_end_matches('/');

        match &self.password {
            Some(password) => format!("redis://:{}@{host}/{}", Self::percent_encoded_password(password), self.database),
            None => format!("redis://{host}/{}", self.database),
        }
    }

    #[must_use]
    fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    #[must_use]
    fn percent_encoded_password(password: &str) -> String {
        let mut encoded_password = String::new();

        for password_byte in password.bytes() {
            if password_byte.is_ascii_alphanumeric() || matches!(password_byte, b'-' | b'.' | b'_' | b'~') {
                encoded_password.push(char::from(password_byte));
            } else {
                encoded_password.push('%');
                encoded_password.push(char::from(b"0123456789ABCDEF"[usize::from(password_byte >> 4)]));
                encoded_password.push(char::from(b"0123456789ABCDEF"[usize::from(password_byte & 0x0f)]));
            }
        }

        encoded_password
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

    #[must_use]
    fn redis_key_fragment(&self) -> String {
        Self::hashed_identifier(&self.identifier)
    }

    #[must_use]
    fn redis_key_pattern(&self, key_prefix: &str) -> String {
        format!("{key_prefix}:{}:*", self.redis_key_fragment())
    }

    #[must_use]
    fn hashed_identifier(identifier: &str) -> String {
        let mut hasher = Sha256::new();

        hasher.update(identifier.as_bytes());

        let hash = hasher.finalize();

        hex_digest(&hash)
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

    #[must_use]
    fn redis_key(&self, key_prefix: &str) -> String {
        let session = AgentCacheSession::hashed_identifier(&self.session_identifier);

        format!("{key_prefix}:{session}:{}", self.agent_hash)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAgentExecution {
    pub output: Value,
    pub context: Value,
}

pub struct RedisAgentCacheStore {
    client: redis::Client,
    config: RedisAgentCacheConfig,
}

impl fmt::Debug for RedisAgentCacheStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisAgentCacheStore")
            .field("host", &self.config.host)
            .field("database", &self.config.database)
            .field("key_prefix", &self.config.key_prefix)
            .finish_non_exhaustive()
    }
}

impl RedisAgentCacheStore {
    pub fn new(config: RedisAgentCacheConfig) -> Result<Self, ExecutorError> {
        let client = redis::Client::open(config.connection_url()).map_err(Self::redis_error)?;

        Ok(Self { client, config })
    }

    fn connection(&self) -> Result<redis::Connection, ExecutorError> {
        self.client.get_connection().map_err(Self::redis_error)
    }

    #[must_use]
    fn time_to_live_milliseconds(time_to_live: Duration) -> u64 {
        u64::try_from(time_to_live.as_millis()).unwrap_or(u64::MAX).max(1)
    }

    #[must_use]
    fn redis_error(error: redis::RedisError) -> ExecutorError {
        ExecutorError::Other {
            message: format!("redis agent cache operation failed: {error}"),
        }
    }

    fn serialization_error(error: serde_json::Error) -> ExecutorError {
        ExecutorError::Other {
            message: format!("failed to serialize redis agent cache value: {error}"),
        }
    }

    fn deserialization_error(error: serde_json::Error) -> ExecutorError {
        ExecutorError::Other {
            message: format!("failed to deserialize redis agent cache value: {error}"),
        }
    }
}

impl AgentCacheStore for RedisAgentCacheStore {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError> {
        let mut connection = self.connection()?;
        let redis_key = key.redis_key(self.config.key_prefix());
        let Some(serialized_execution) = connection.get::<_, Option<Vec<u8>>>(redis_key).map_err(Self::redis_error)? else {
            return Ok(None);
        };

        serde_json::from_slice(&serialized_execution)
            .map(Some)
            .map_err(Self::deserialization_error)
    }

    fn put(&self, key: AgentCacheKey, execution: CachedAgentExecution, time_to_live: Duration) -> Result<(), ExecutorError> {
        if time_to_live.is_zero() {
            return Ok(());
        }

        let mut connection = self.connection()?;
        let redis_key = key.redis_key(self.config.key_prefix());
        let serialized_execution = serde_json::to_vec(&execution).map_err(Self::serialization_error)?;

        redis::cmd("PSETEX")
            .arg(redis_key)
            .arg(Self::time_to_live_milliseconds(time_to_live))
            .arg(serialized_execution)
            .query::<()>(&mut connection)
            .map_err(Self::redis_error)
    }

    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError> {
        let mut connection = self.connection()?;
        let key_pattern = session.redis_key_pattern(self.config.key_prefix());
        let mut cursor = 0_u64;
        let mut deleted_count = 0_usize;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&key_pattern)
                .arg("COUNT")
                .arg(1000_u16)
                .query(&mut connection)
                .map_err(Self::redis_error)?;

            if !keys.is_empty() {
                let batch_deleted_count = redis::cmd("DEL")
                    .arg(keys)
                    .query::<usize>(&mut connection)
                    .map_err(Self::redis_error)?;

                deleted_count += batch_deleted_count;
            }

            if next_cursor == 0 {
                break;
            }

            cursor = next_cursor;
        }

        Ok(deleted_count)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_config_encodes_password_in_connection_url() {
        let config = RedisAgentCacheConfig::new("redis.internal:6380", Some("p@ss word".to_string()), 2);

        assert_eq!(config.connection_url(), "redis://:p%40ss%20word@redis.internal:6380/2");
    }

    #[test]
    fn redis_config_omits_empty_password() {
        let config = RedisAgentCacheConfig::new("redis.internal:6380", Some(String::new()), 0);

        assert_eq!(config.connection_url(), "redis://redis.internal:6380/0");
    }

    #[test]
    fn redis_keys_use_hashed_sessions() {
        let session = AgentCacheSession::new("browser-a");
        let key = AgentCacheKey::new(&session, "agent-hash".to_string());

        assert_eq!(
            key.redis_key("superwire:test"),
            format!("superwire:test:{}:agent-hash", session.redis_key_fragment())
        );
        assert_eq!(
            session.redis_key_pattern("superwire:test"),
            format!("superwire:test:{}:*", session.redis_key_fragment())
        );
    }
}
