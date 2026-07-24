use crate::runtime::ExecutorError;
use redis::Commands;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use superwire_protocol::event::CacheOperation;

pub const DEFAULT_AGENT_CACHE_TIME_TO_LIVE: Duration = Duration::from_secs(60 * 60);
pub const LOCAL_AGENT_CACHE_SESSION: &str = "local";
pub const DEFAULT_IN_MEMORY_AGENT_CACHE_MAX_ENTRIES: usize = 1024;
pub const DEFAULT_IN_MEMORY_AGENT_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

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
    in_memory: InMemoryAgentCacheConfig,
    redis: RedisAgentCacheConfig,
}

impl AgentCacheConfig {
    #[must_use]
    pub fn new(driver: AgentCacheDriver) -> Self {
        Self {
            driver,
            in_memory: InMemoryAgentCacheConfig::default(),
            redis: RedisAgentCacheConfig::default(),
        }
    }

    #[must_use]
    pub fn with_in_memory(mut self, in_memory: InMemoryAgentCacheConfig) -> Self {
        self.in_memory = in_memory;

        self
    }

    #[must_use]
    pub fn with_redis(mut self, redis: RedisAgentCacheConfig) -> Self {
        self.redis = redis;

        self
    }

    pub fn build_store(&self) -> Result<Arc<dyn AgentCacheStore>, ExecutorError> {
        match self.driver {
            AgentCacheDriver::InMemory => Ok(Arc::new(InMemoryAgentCacheStore::new(self.in_memory))),
            AgentCacheDriver::Redis => {
                RedisAgentCacheStore::new(self.redis.clone()).map(|store| Arc::new(store) as Arc<dyn AgentCacheStore>)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InMemoryAgentCacheConfig {
    max_entries: usize,
    max_bytes: usize,
}

impl InMemoryAgentCacheConfig {
    #[must_use]
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self { max_entries, max_bytes }
    }

    #[must_use]
    pub fn max_entries(self) -> usize {
        self.max_entries
    }

    #[must_use]
    pub fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

impl Default for InMemoryAgentCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_IN_MEMORY_AGENT_CACHE_MAX_ENTRIES,
            max_bytes: DEFAULT_IN_MEMORY_AGENT_CACHE_MAX_BYTES,
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
    fn in_memory_size_bytes(&self) -> usize {
        self.session_identifier.len().saturating_add(self.agent_hash.len())
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
        let client = redis::Client::open(config.connection_url()).map_err(|error| Self::redis_error(CacheOperation::Connect, error))?;

        Ok(Self { client, config })
    }

    fn connection(&self, operation: CacheOperation) -> Result<redis::Connection, ExecutorError> {
        self.client.get_connection().map_err(|error| Self::redis_error(operation, error))
    }

    #[must_use]
    fn time_to_live_milliseconds(time_to_live: Duration) -> u64 {
        u64::try_from(time_to_live.as_millis()).unwrap_or(u64::MAX).max(1)
    }

    #[must_use]
    fn redis_error(operation: CacheOperation, error: redis::RedisError) -> ExecutorError {
        ExecutorError::cache_with_source(operation, format!("redis agent cache operation failed: {error}"), error)
    }

    fn serialization_error(error: serde_json::Error) -> ExecutorError {
        ExecutorError::cache_with_source(
            CacheOperation::Write,
            format!("failed to serialize redis agent cache value: {error}"),
            error,
        )
    }

    fn deserialization_error(error: serde_json::Error) -> ExecutorError {
        ExecutorError::cache_with_source(
            CacheOperation::Read,
            format!("failed to deserialize redis agent cache value: {error}"),
            error,
        )
    }
}

impl AgentCacheStore for RedisAgentCacheStore {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError> {
        let mut connection = self.connection(CacheOperation::Read)?;
        let redis_key = key.redis_key(self.config.key_prefix());
        let Some(serialized_execution) = connection
            .get::<_, Option<Vec<u8>>>(redis_key)
            .map_err(|error| Self::redis_error(CacheOperation::Read, error))?
        else {
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

        let mut connection = self.connection(CacheOperation::Write)?;
        let redis_key = key.redis_key(self.config.key_prefix());
        let serialized_execution = serde_json::to_vec(&execution).map_err(Self::serialization_error)?;

        redis::cmd("PSETEX")
            .arg(redis_key)
            .arg(Self::time_to_live_milliseconds(time_to_live))
            .arg(serialized_execution)
            .query::<()>(&mut connection)
            .map_err(|error| Self::redis_error(CacheOperation::Write, error))
    }

    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError> {
        let mut connection = self.connection(CacheOperation::Purge)?;
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
                .map_err(|error| Self::redis_error(CacheOperation::Purge, error))?;

            if !keys.is_empty() {
                let batch_deleted_count = redis::cmd("DEL")
                    .arg(keys)
                    .query::<usize>(&mut connection)
                    .map_err(|error| Self::redis_error(CacheOperation::Purge, error))?;

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

    fn serialized_size_bytes(&self) -> Result<usize, ExecutorError> {
        let mut size_writer = SerializedSizeWriter::default();

        serde_json::to_writer(&mut size_writer, self).map_err(|error| {
            ExecutorError::cache_with_source(
                CacheOperation::Write,
                format!("failed to measure in-memory agent cache value: {error}"),
                error,
            )
        })?;

        Ok(size_writer.bytes_written)
    }
}

pub trait AgentCacheStore: Send + Sync + fmt::Debug {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError>;
    fn put(&self, key: AgentCacheKey, execution: CachedAgentExecution, time_to_live: Duration) -> Result<(), ExecutorError>;
    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError>;
}

pub struct InMemoryAgentCacheStore {
    state: Mutex<InMemoryAgentCacheState>,
    config: InMemoryAgentCacheConfig,
}

impl fmt::Debug for InMemoryAgentCacheStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (entry_count, total_bytes) = self
            .state
            .lock()
            .map(|state| (state.entries.len(), state.total_bytes))
            .unwrap_or_default();

        formatter
            .debug_struct("InMemoryAgentCacheStore")
            .field("max_entries", &self.config.max_entries())
            .field("max_bytes", &self.config.max_bytes())
            .field("entry_count", &entry_count)
            .field("total_bytes", &total_bytes)
            .finish()
    }
}

impl Default for InMemoryAgentCacheStore {
    fn default() -> Self {
        Self::new(InMemoryAgentCacheConfig::default())
    }
}

impl InMemoryAgentCacheStore {
    #[must_use]
    pub fn new(config: InMemoryAgentCacheConfig) -> Self {
        Self {
            state: Mutex::new(InMemoryAgentCacheState::default()),
            config,
        }
    }
}

#[derive(Debug, Default)]
struct InMemoryAgentCacheState {
    entries: HashMap<AgentCacheKey, InMemoryAgentCacheEntry>,
    total_bytes: usize,
    next_access_sequence: u64,
}

impl InMemoryAgentCacheState {
    fn sweep_expired(&mut self, now: Instant) {
        let mut expired_bytes = 0_usize;

        self.entries.retain(|_key, entry| {
            if !entry.is_expired(now) {
                return true;
            }

            expired_bytes = expired_bytes.saturating_add(entry.size_bytes);

            false
        });
        self.total_bytes = self.total_bytes.saturating_sub(expired_bytes);
    }

    fn next_access_sequence(&mut self) -> u64 {
        self.next_access_sequence = self.next_access_sequence.saturating_add(1);

        self.next_access_sequence
    }

    fn remove(&mut self, key: &AgentCacheKey) -> Option<InMemoryAgentCacheEntry> {
        let removed_entry = self.entries.remove(key)?;

        self.total_bytes = self.total_bytes.saturating_sub(removed_entry.size_bytes);

        Some(removed_entry)
    }

    fn enforce_limits(&mut self, config: InMemoryAgentCacheConfig) {
        while self.entries.len() > config.max_entries() || self.total_bytes > config.max_bytes() {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_key, entry)| entry.last_access_sequence)
                .map(|(key, _entry)| key.clone())
            else {
                break;
            };

            self.remove(&oldest_key);
        }
    }
}

#[derive(Debug, Clone)]
struct InMemoryAgentCacheEntry {
    execution: CachedAgentExecution,
    expires_at: Instant,
    last_access_sequence: u64,
    size_bytes: usize,
}

impl InMemoryAgentCacheEntry {
    #[must_use]
    fn new(execution: CachedAgentExecution, time_to_live: Duration, last_access_sequence: u64, size_bytes: usize) -> Self {
        Self {
            execution,
            expires_at: Instant::now() + time_to_live,
            last_access_sequence,
            size_bytes,
        }
    }

    #[must_use]
    fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

impl AgentCacheStore for InMemoryAgentCacheStore {
    fn get(&self, key: &AgentCacheKey) -> Result<Option<CachedAgentExecution>, ExecutorError> {
        let mut state = self
            .state
            .lock()
            .map_err(|error| ExecutorError::cache(CacheOperation::Read, format!("failed to acquire agent cache lock: {error}")))?;

        state.sweep_expired(Instant::now());
        let access_sequence = state.next_access_sequence();
        let Some(entry) = state.entries.get_mut(key) else {
            return Ok(None);
        };

        entry.last_access_sequence = access_sequence;

        Ok(Some(entry.execution.clone()))
    }

    fn put(&self, key: AgentCacheKey, execution: CachedAgentExecution, time_to_live: Duration) -> Result<(), ExecutorError> {
        let execution_size_bytes = execution.serialized_size_bytes()?;
        let entry_size_bytes = key.in_memory_size_bytes().saturating_add(execution_size_bytes);
        let mut state = self
            .state
            .lock()
            .map_err(|error| ExecutorError::cache(CacheOperation::Write, format!("failed to acquire agent cache lock: {error}")))?;

        state.sweep_expired(Instant::now());
        state.remove(&key);

        if time_to_live.is_zero() || self.config.max_entries() == 0 || entry_size_bytes > self.config.max_bytes() {
            return Ok(());
        }

        let access_sequence = state.next_access_sequence();

        state.total_bytes = state.total_bytes.saturating_add(entry_size_bytes);
        state.entries.insert(
            key,
            InMemoryAgentCacheEntry::new(execution, time_to_live, access_sequence, entry_size_bytes),
        );
        state.enforce_limits(self.config);

        Ok(())
    }

    fn purge_session(&self, session: &AgentCacheSession) -> Result<usize, ExecutorError> {
        let mut state = self
            .state
            .lock()
            .map_err(|error| ExecutorError::cache(CacheOperation::Purge, format!("failed to acquire agent cache lock: {error}")))?;

        state.sweep_expired(Instant::now());
        let previous_entry_count = state.entries.len();

        let mut purged_bytes = 0_usize;

        state.entries.retain(|key, entry| {
            if key.session_identifier() != session.identifier() {
                return true;
            }

            purged_bytes = purged_bytes.saturating_add(entry.size_bytes);

            false
        });
        state.total_bytes = state.total_bytes.saturating_sub(purged_bytes);

        Ok(previous_entry_count.saturating_sub(state.entries.len()))
    }
}

#[derive(Debug, Default)]
struct SerializedSizeWriter {
    bytes_written: usize,
}

impl Write for SerializedSizeWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes_written = self
            .bytes_written
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::other("serialized cache value size overflowed usize"))?;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn hash_serializable_value(value: &impl serde::Serialize) -> Result<String, ExecutorError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| ExecutorError::internal_with_source(format!("failed to serialize agent cache fingerprint: {error}"), error))?;
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
    use serde_json::json;

    fn test_cache_key(agent_hash: &str) -> AgentCacheKey {
        AgentCacheKey::new(&AgentCacheSession::new("test-session"), agent_hash.to_string())
    }

    fn test_cached_execution(value: &str) -> CachedAgentExecution {
        CachedAgentExecution::new(json!({ "value": value }), Value::Null)
    }

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

    #[test]
    fn in_memory_cache_sweeps_expired_entries_on_get_and_put() {
        let cache_store = InMemoryAgentCacheStore::new(InMemoryAgentCacheConfig::new(10, 1024 * 1024));
        let expired_on_get_key = test_cache_key("expired-on-get");

        cache_store
            .put(
                expired_on_get_key.clone(),
                test_cached_execution("expired"),
                Duration::from_secs(60),
            )
            .expect("cache put should succeed");

        {
            let mut state = cache_store.state.lock().expect("cache state should lock");
            let entry = state.entries.get_mut(&expired_on_get_key).expect("cached entry should exist");

            entry.expires_at = Instant::now();
        }

        assert!(cache_store.get(&expired_on_get_key).expect("cache get should succeed").is_none());

        let expired_on_put_key = test_cache_key("expired-on-put");
        let fresh_key = test_cache_key("fresh");

        cache_store
            .put(
                expired_on_put_key.clone(),
                test_cached_execution("expired"),
                Duration::from_secs(60),
            )
            .expect("cache put should succeed");

        {
            let mut state = cache_store.state.lock().expect("cache state should lock");
            let entry = state.entries.get_mut(&expired_on_put_key).expect("cached entry should exist");

            entry.expires_at = Instant::now();
        }

        cache_store
            .put(fresh_key.clone(), test_cached_execution("fresh"), Duration::from_secs(60))
            .expect("cache put should sweep expired entries");

        let state = cache_store.state.lock().expect("cache state should lock");

        assert_eq!(state.entries.len(), 1);
        assert!(state.entries.contains_key(&fresh_key));
    }

    #[test]
    fn in_memory_cache_evicts_least_recently_used_entry() {
        let cache_store = InMemoryAgentCacheStore::new(InMemoryAgentCacheConfig::new(2, 1024 * 1024));
        let first_key = test_cache_key("first");
        let second_key = test_cache_key("second");
        let third_key = test_cache_key("third");

        cache_store
            .put(first_key.clone(), test_cached_execution("first"), Duration::from_secs(60))
            .expect("first cache put should succeed");
        cache_store
            .put(second_key.clone(), test_cached_execution("second"), Duration::from_secs(60))
            .expect("second cache put should succeed");
        cache_store
            .get(&first_key)
            .expect("cache get should succeed")
            .expect("first cache entry should exist");
        cache_store
            .put(third_key.clone(), test_cached_execution("third"), Duration::from_secs(60))
            .expect("third cache put should evict the least recently used entry");

        assert!(cache_store.get(&first_key).expect("first cache get should succeed").is_some());
        assert!(cache_store.get(&second_key).expect("second cache get should succeed").is_none());
        assert!(cache_store.get(&third_key).expect("third cache get should succeed").is_some());
    }

    #[test]
    fn in_memory_cache_skips_entries_larger_than_byte_limit() {
        let cache_store = InMemoryAgentCacheStore::new(InMemoryAgentCacheConfig::new(10, 1));
        let oversized_key = test_cache_key("oversized");

        cache_store
            .put(oversized_key.clone(), test_cached_execution("too-large"), Duration::from_secs(60))
            .expect("oversized cache put should degrade to a miss");

        assert!(cache_store.get(&oversized_key).expect("cache get should succeed").is_none());

        let state = cache_store.state.lock().expect("cache state should lock");

        assert_eq!(state.total_bytes, 0);
        assert!(state.entries.is_empty());
    }
}
