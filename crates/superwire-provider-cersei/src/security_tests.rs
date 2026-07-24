use super::{
    CerseiModelProvider, DetachedProviderFileCleanup, FileProviderOperation, FileUploadClient, InferenceParameter, ProviderConfigCerseiExt,
    ProviderFileCleanupExecutor, ProviderFileCleanupScheduleOutcome, ProviderNetworkPolicy, ProviderRetryContext, UploadedProviderFile,
};
use async_trait::async_trait;
use cersei_types::CerseiError;
use log::{LevelFilter, Log, Metadata, Record};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use superwire_mcp::McpClientPool;
use superwire_model::{ModelFileAttachment, ModelProvider, ModelRequest, ModelSchema, ModelToolDefinition, ToolCallTracker};
use superwire_protocol::event::{DiagnosticRetryability, ExecutorDiagnosticCode, ExecutorDiagnosticSubject};
use superwire_semantic::support::provider::{ProviderConfig, ProviderDriver};
use superwire_types::ModelWireApi;

const AMBIENT_API_KEY: &str = "ambient-key-must-never-reach-custom-endpoint";
const EXPLICIT_API_KEY: &str = "explicit-key-for-custom-endpoint";
const HOSTILE_BODY_SECRET: &str = "reflected-upstream-body-secret";
const HOSTILE_PROMPT_SECRET: &str = "private-prompt-fragment";
const HOSTILE_QUERY_SECRET: &str = "signed-query-token";
const ENVIRONMENT_VARIABLES: [&str; 17] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_KEY",
    "OPENAI_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "MISTRAL_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "XAI_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "PERPLEXITY_API_KEY",
    "CEREBRAS_API_KEY",
    "OPENROUTER_API_KEY",
    "COHERE_API_KEY",
    "CO_API_KEY",
    "SAMBANOVA_API_KEY",
];
static ENVIRONMENT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static LOG_INITIALIZATION: Once = Once::new();
static CAPTURED_LOGGER: CapturedLogger = CapturedLogger {
    records: Mutex::new(Vec::new()),
};

#[derive(Clone, Copy)]
struct DriverSecurityCase {
    driver: ProviderDriver,
    uses_ambient_api_key: bool,
    permits_missing_api_key: bool,
}

const DRIVER_SECURITY_CASES: [DriverSecurityCase; 17] = [
    DriverSecurityCase {
        driver: ProviderDriver::Anthropic,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::OpenAi,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Google,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Mistral,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Groq,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::DeepSeek,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Xai,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Together,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Fireworks,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Perplexity,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Cerebras,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Ollama,
        uses_ambient_api_key: false,
        permits_missing_api_key: true,
    },
    DriverSecurityCase {
        driver: ProviderDriver::OpenRouter,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::Cohere,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::SambaNova,
        uses_ambient_api_key: true,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::OpenAiCompatible,
        uses_ambient_api_key: false,
        permits_missing_api_key: false,
    },
    DriverSecurityCase {
        driver: ProviderDriver::AnthropicCompatible,
        uses_ambient_api_key: false,
        permits_missing_api_key: false,
    },
];

#[tokio::test]
async fn provider_security_boundaries_hold_for_all_drivers() {
    let _environment_lock = ENVIRONMENT_LOCK.lock().await;
    let _environment_guard = EnvironmentGuard::set_all(AMBIENT_API_KEY);

    CAPTURED_LOGGER.enable_and_clear();

    assert_eq!(ProviderDriver::all().len(), DRIVER_SECURITY_CASES.len());

    let capture_server = CaptureServer::spawn(format!("{{\"error\":{{\"message\":\"{HOSTILE_BODY_SECRET}\"}}}}"));

    for driver_case in DRIVER_SECURITY_CASES {
        driver_case.assert_credential_resolution_matrix(capture_server.endpoint()).await;
    }

    assert_custom_endpoint_request_isolation(&capture_server).await;

    let endpoint_with_sensitive_components = emit_endpoint_configuration_log(&capture_server);
    let serialized_diagnostic = assert_hostile_diagnostic_is_sanitized(&capture_server).await;

    assert_file_failures_are_sanitized(&capture_server).await;
    assert_public_output_excludes_secrets(&serialized_diagnostic, &endpoint_with_sensitive_components, "public diagnostic");

    let captured_logs = CAPTURED_LOGGER.contents();

    assert!(captured_logs.contains("building Cersei provider"));
    assert_public_output_excludes_secrets(&captured_logs, &endpoint_with_sensitive_components, "logs");
}

async fn assert_custom_endpoint_request_isolation(capture_server: &CaptureServer) {
    let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);

    for driver_case in DRIVER_SECURITY_CASES {
        let request_count_before_missing_key = capture_server.request_count();
        let missing_key_result = model_provider
            .generate(model_request(
                driver_case.driver,
                capture_server.endpoint().to_string(),
                None,
                "credential isolation probe",
                0,
            ))
            .await;

        if driver_case.permits_missing_api_key {
            assert!(
                missing_key_result.is_err(),
                "capture server should reject {}",
                driver_case.driver.as_str()
            );
            assert_eq!(capture_server.request_count(), request_count_before_missing_key + 1);
            assert_request_excludes_secret(&capture_server.latest_request(), AMBIENT_API_KEY);
        } else {
            assert!(
                missing_key_result.is_err(),
                "{} should require an explicit key",
                driver_case.driver.as_str()
            );
            assert_eq!(
                capture_server.request_count(),
                request_count_before_missing_key,
                "{} sent a request using an ambient credential",
                driver_case.driver.as_str()
            );
        }

        let request_count_before_explicit_key = capture_server.request_count();
        let explicit_key_result = model_provider
            .generate(model_request(
                driver_case.driver,
                capture_server.endpoint().to_string(),
                Some(EXPLICIT_API_KEY.to_string()),
                "credential isolation probe",
                0,
            ))
            .await;

        assert!(
            explicit_key_result.is_err(),
            "capture server should reject {}",
            driver_case.driver.as_str()
        );
        assert_eq!(capture_server.request_count(), request_count_before_explicit_key + 1);

        let explicit_request = capture_server.latest_request();

        assert!(
            explicit_request.contains(EXPLICIT_API_KEY),
            "{} did not send its explicit key through its provider-specific authentication mechanism",
            driver_case.driver.as_str()
        );
        assert_request_excludes_secret(&explicit_request, AMBIENT_API_KEY);
    }
}

fn emit_endpoint_configuration_log(capture_server: &CaptureServer) -> String {
    let endpoint_with_sensitive_components = format!("{}/v1?signature={HOSTILE_QUERY_SECRET}#private-fragment", capture_server.endpoint());
    let endpoint_log_request = model_request(
        ProviderDriver::OpenAi,
        endpoint_with_sensitive_components.clone(),
        Some(EXPLICIT_API_KEY.to_string()),
        "endpoint log probe",
        0,
    );

    let _endpoint_log_request = endpoint_log_request;

    endpoint_with_sensitive_components
}

async fn assert_hostile_diagnostic_is_sanitized(capture_server: &CaptureServer) -> String {
    let hostile_diagnostic_request = model_request(
        ProviderDriver::OpenAiCompatible,
        capture_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        HOSTILE_PROMPT_SECRET,
        1,
    );
    let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);

    let hostile_diagnostic_error = model_provider
        .generate(hostile_diagnostic_request)
        .await
        .expect_err("hostile provider response should fail after its retry");
    let hostile_diagnostic = hostile_diagnostic_error.diagnostic();
    let serialized_diagnostic = serde_json::to_string(hostile_diagnostic).expect("diagnostic should serialize");

    assert_eq!(hostile_diagnostic.code, ExecutorDiagnosticCode::ProviderRetriesExhausted);
    assert_eq!(hostile_diagnostic.retryability, DiagnosticRetryability::Safe);

    let final_attempt = hostile_diagnostic
        .cause
        .as_deref()
        .expect("retry exhaustion should retain a sanitized final-attempt diagnostic");
    let ExecutorDiagnosticSubject::Provider { attempt, http_status, .. } = &final_attempt.subject else {
        panic!("final attempt should retain provider metadata");
    };

    assert_eq!(*attempt, Some(2));
    assert_eq!(*http_status, Some(500));
    assert_eq!(final_attempt.message, "provider service failed");
    assert_eq!(final_attempt.retryability, DiagnosticRetryability::Safe);
    assert!(Error::source(&hostile_diagnostic_error).is_some());

    serialized_diagnostic
}

async fn assert_file_failures_are_sanitized(capture_server: &CaptureServer) {
    let request = model_request(
        ProviderDriver::OpenAiCompatible,
        capture_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "file failure probe",
        0,
    );
    let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
    let endpoint_approval = model_provider
        .approve_endpoint(&request)
        .await
        .expect("trusted file endpoint should be approved");
    let file_upload_client =
        FileUploadClient::from_request(&request, &endpoint_approval).expect("file client should use approved HTTP client");
    let file_attachment = ModelFileAttachment {
        name: "sensitive.txt".to_string(),
        content: HOSTILE_PROMPT_SECRET.to_string(),
        purpose: "file-extract".to_string(),
    };
    let upload_error = file_upload_client
        .upload(&file_attachment, "security-agent")
        .await
        .expect_err("hostile upload response should fail");
    let uploaded_file = UploadedProviderFile {
        id: HOSTILE_PROMPT_SECRET.to_string(),
        filename: "sensitive.txt".to_string(),
        purpose: "file-extract".to_string(),
        bytes: None,
    };
    let delete_error = file_upload_client
        .delete(&uploaded_file.id, "security-agent")
        .await
        .expect_err("hostile delete response should fail");

    assert_file_failure(upload_error.diagnostic(), FileProviderOperation::Upload);
    assert_file_failure(delete_error.diagnostic(), FileProviderOperation::Delete);
}

fn assert_public_output_excludes_secrets(output: &str, endpoint_with_sensitive_components: &str, output_name: &str) {
    for forbidden_value in [
        AMBIENT_API_KEY,
        EXPLICIT_API_KEY,
        HOSTILE_BODY_SECRET,
        HOSTILE_PROMPT_SECRET,
        HOSTILE_QUERY_SECRET,
        endpoint_with_sensitive_components,
    ] {
        assert!(!output.contains(forbidden_value), "{output_name} leaked `{forbidden_value}`");
    }
}

#[test]
fn provider_error_variants_use_fixed_public_messages() {
    let request = model_request(
        ProviderDriver::OpenAiCompatible,
        "http://127.0.0.1:1".to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        HOSTILE_PROMPT_SECRET,
        0,
    );
    let retry_context = ProviderRetryContext::new(&request);
    let invalid_json = serde_json::from_str::<Value>("{ invalid json").expect_err("fixture should be invalid JSON");
    let errors_and_messages = vec![
        (
            CerseiError::ProviderStatus {
                status: 401,
                message: HOSTILE_BODY_SECRET.to_string(),
            },
            "provider authentication failed",
        ),
        (
            CerseiError::ProviderStatus {
                status: 403,
                message: HOSTILE_BODY_SECRET.to_string(),
            },
            "provider permission denied",
        ),
        (
            CerseiError::ProviderStatus {
                status: 429,
                message: HOSTILE_BODY_SECRET.to_string(),
            },
            "provider rate limit exceeded",
        ),
        (
            CerseiError::ProviderStatus {
                status: 503,
                message: HOSTILE_BODY_SECRET.to_string(),
            },
            "provider service failed",
        ),
        (
            CerseiError::Provider(format!("HTTP 502: {HOSTILE_BODY_SECRET}")),
            "provider service failed",
        ),
        (CerseiError::Auth(HOSTILE_BODY_SECRET.to_string()), "provider authentication failed"),
        (
            CerseiError::Tool(HOSTILE_BODY_SECRET.to_string()),
            "provider tool processing failed",
        ),
        (
            CerseiError::Permission(HOSTILE_BODY_SECRET.to_string()),
            "provider permission denied",
        ),
        (
            CerseiError::ContextOverflow { used: 20, limit: 10 },
            "provider context limit exceeded",
        ),
        (
            CerseiError::Config(HOSTILE_BODY_SECRET.to_string()),
            "provider configuration was rejected",
        ),
        (CerseiError::Mcp(HOSTILE_BODY_SECRET.to_string()), "provider MCP operation failed"),
        (CerseiError::Json(invalid_json), "provider response was invalid JSON"),
        (
            CerseiError::Io(std::io::Error::other(HOSTILE_BODY_SECRET)),
            "provider I/O operation failed",
        ),
        (
            CerseiError::Other(std::io::Error::other(HOSTILE_BODY_SECRET).into()),
            "provider request failed",
        ),
        (CerseiError::Cancelled, "provider request was cancelled"),
        (
            CerseiError::RateLimit {
                retry_after: Some(Duration::from_secs(3)),
            },
            "provider rate limit exceeded",
        ),
    ];

    for (error, expected_message) in errors_and_messages {
        let diagnostic = retry_context.failure_diagnostic(&error, 4);
        let serialized_diagnostic = serde_json::to_string(&diagnostic).expect("diagnostic should serialize");

        assert_eq!(diagnostic.message, expected_message);
        assert!(!serialized_diagnostic.contains(HOSTILE_BODY_SECRET));
        assert!(!serialized_diagnostic.contains(HOSTILE_PROMPT_SECRET));
    }
}

#[derive(Debug, Default)]
struct TestProviderDnsResolver {
    addresses_by_hostname: BTreeMap<String, Vec<IpAddr>>,
    requested_hostnames: Arc<Mutex<Vec<String>>>,
}

impl TestProviderDnsResolver {
    fn with_host(hostname: &str, addresses: impl IntoIterator<Item = IpAddr>) -> Self {
        Self {
            addresses_by_hostname: [(hostname.to_string(), addresses.into_iter().collect())].into(),
            requested_hostnames: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn request_count(&self) -> usize {
        self.requested_hostnames
            .lock()
            .expect("DNS request records lock should not be poisoned")
            .len()
    }
}

#[async_trait]
impl super::network::ProviderDnsResolver for TestProviderDnsResolver {
    async fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        self.requested_hostnames
            .lock()
            .expect("DNS request records lock should not be poisoned")
            .push(hostname.to_string());

        Ok(self
            .addresses_by_hostname
            .get(hostname)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|ip_address| SocketAddr::new(ip_address, port))
            .collect())
    }
}

#[tokio::test]
async fn provider_network_policy_defaults_to_exact_built_in_endpoints() {
    assert_eq!(
        "built-in-only".parse::<ProviderNetworkPolicy>(),
        Ok(ProviderNetworkPolicy::BuiltInOnly)
    );
    assert_eq!(
        "public-only".parse::<ProviderNetworkPolicy>(),
        Ok(ProviderNetworkPolicy::PublicOnly)
    );
    assert_eq!("trusted".parse::<ProviderNetworkPolicy>(), Ok(ProviderNetworkPolicy::Trusted));
    assert!("unrestricted".parse::<ProviderNetworkPolicy>().is_err());

    let listener = TcpListener::bind("127.0.0.1:0").expect("policy test listener should bind");
    listener
        .set_nonblocking(true)
        .expect("policy test listener should become nonblocking");
    let loopback_endpoint = format!("http://{}", listener.local_addr().expect("policy listener address should exist"));
    let loopback_request = model_request(
        ProviderDriver::OpenAiCompatible,
        loopback_endpoint,
        Some(EXPLICIT_API_KEY.to_string()),
        "policy probe",
        0,
    );
    let rejection = CerseiModelProvider::default()
        .generate(loopback_request)
        .await
        .expect_err("default policy must reject a workflow-controlled loopback endpoint");

    assert_eq!(rejection.diagnostic().message, "custom provider endpoints are disabled");
    assert_eq!(
        listener
            .accept()
            .expect_err("built-in-only policy must reject before opening a socket")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );

    let built_in_resolver = Arc::new(TestProviderDnsResolver::default());
    let built_in_provider =
        CerseiModelProvider::for_network_policy_and_dns_resolver(ProviderNetworkPolicy::BuiltInOnly, built_in_resolver.clone());
    let mut built_in_request = model_request(
        ProviderDriver::OpenAi,
        ProviderDriver::OpenAi
            .default_endpoint()
            .expect("OpenAI should have a built-in endpoint")
            .to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "built-in policy probe",
        0,
    );
    built_in_request.provider_config.endpoint = None;

    built_in_provider
        .approve_endpoint(&built_in_request)
        .await
        .expect("exact built-in endpoint should be approved without DNS");
    assert_eq!(built_in_resolver.request_count(), 0);
}

#[tokio::test]
async fn public_only_provider_policy_rejects_private_dns_and_ipv4_addresses() {
    let public_resolver = Arc::new(TestProviderDnsResolver::with_host(
        "provider.example",
        [IpAddr::V4(Ipv4Addr::LOCALHOST)],
    ));
    let public_provider =
        CerseiModelProvider::for_network_policy_and_dns_resolver(ProviderNetworkPolicy::PublicOnly, public_resolver.clone());
    let public_request = model_request(
        ProviderDriver::OpenAiCompatible,
        "https://provider.example/v1".to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "public policy probe",
        0,
    );

    public_provider
        .approve_endpoint(&public_request)
        .await
        .expect_err("public-only policy must reject private DNS answers");
    assert_eq!(public_resolver.request_count(), 1);

    for forbidden_address in [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "100.100.100.200",
        "127.0.0.1",
        "168.63.129.16",
        "169.254.169.254",
        "172.16.0.1",
        "192.168.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "255.255.255.255",
    ] {
        let forbidden_request = model_request(
            ProviderDriver::OpenAiCompatible,
            format!("https://{forbidden_address}/v1"),
            Some(EXPLICIT_API_KEY.to_string()),
            "IPv4 policy probe",
            0,
        );

        public_provider
            .approve_endpoint(&forbidden_request)
            .await
            .expect_err("public-only policy must reject non-public IPv4 addresses");
    }
}

#[tokio::test]
async fn public_only_provider_policy_rejects_ipv6_and_unsafe_url_syntax() {
    let public_resolver = Arc::new(TestProviderDnsResolver::default());
    let public_provider =
        CerseiModelProvider::for_network_policy_and_dns_resolver(ProviderNetworkPolicy::PublicOnly, public_resolver.clone());

    for forbidden_address in [
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "64:ff9b::1",
        "fc00::1",
        "fd00:ec2::254",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "2002:7f00:1::",
        "3fff::1",
    ] {
        let forbidden_request = model_request(
            ProviderDriver::OpenAiCompatible,
            format!("https://[{forbidden_address}]/v1"),
            Some(EXPLICIT_API_KEY.to_string()),
            "IPv6 policy probe",
            0,
        );

        public_provider
            .approve_endpoint(&forbidden_request)
            .await
            .expect_err("public-only policy must reject non-public IPv6 addresses");
    }

    for public_endpoint in ["https://8.8.8.8/v1", "https://[2606:4700:4700::1111]/v1"] {
        let public_address_request = model_request(
            ProviderDriver::OpenAiCompatible,
            public_endpoint.to_string(),
            Some(EXPLICIT_API_KEY.to_string()),
            "public address policy probe",
            0,
        );

        public_provider
            .approve_endpoint(&public_address_request)
            .await
            .expect("public-only policy should accept public IP literals");
    }

    assert_eq!(public_resolver.request_count(), 0);

    for rejected_endpoint in [
        "http://8.8.8.8/v1",
        "https://credential@8.8.8.8/v1",
        "https://8.8.8.8/v1?token=private",
        "https://8.8.8.8/v1#private",
        "https://localhost/v1",
        "https://localhost./v1",
        "file:///tmp/provider",
    ] {
        let rejected_request = model_request(
            ProviderDriver::OpenAiCompatible,
            rejected_endpoint.to_string(),
            Some(EXPLICIT_API_KEY.to_string()),
            "endpoint syntax probe",
            0,
        );

        public_provider
            .approve_endpoint(&rejected_request)
            .await
            .expect_err("unsafe provider endpoint syntax should be rejected");
    }

    assert_eq!(public_resolver.request_count(), 0);
}

#[tokio::test]
async fn trusted_provider_policy_pins_dns_and_rejects_redirects() {
    let capture_server = CaptureServer::spawn("{}".to_string());
    let capture_url = reqwest::Url::parse(capture_server.endpoint()).expect("capture endpoint should parse");
    let capture_port = capture_url.port().expect("capture endpoint should include a port");
    let trusted_resolver = Arc::new(TestProviderDnsResolver::with_host(
        "provider.example",
        [IpAddr::V4(Ipv4Addr::LOCALHOST)],
    ));
    let trusted_provider =
        CerseiModelProvider::for_network_policy_and_dns_resolver(ProviderNetworkPolicy::Trusted, trusted_resolver.clone());
    let pinned_endpoint = format!("http://provider.example:{capture_port}");
    let pinned_request = model_request(
        ProviderDriver::OpenAiCompatible,
        pinned_endpoint.clone(),
        Some(EXPLICIT_API_KEY.to_string()),
        "DNS pin probe",
        0,
    );
    let endpoint_approval = trusted_provider
        .approve_endpoint(&pinned_request)
        .await
        .expect("trusted endpoint should be approved");
    let response = endpoint_approval
        .http_client()
        .get(pinned_endpoint)
        .send()
        .await
        .expect("pinned provider request should reach the approved address");

    assert_eq!(response.status(), reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(trusted_resolver.request_count(), 1);
    assert_eq!(capture_server.request_count(), 1);

    let redirect_target = CaptureServer::spawn("{}".to_string());
    let redirect_source = CaptureServer::spawn_redirect(format!("{}/redirected", redirect_target.endpoint()));
    let redirect_request = model_request(
        ProviderDriver::OpenAiCompatible,
        redirect_source.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "redirect probe",
        0,
    );
    let redirect_approval = trusted_provider
        .approve_endpoint(&redirect_request)
        .await
        .expect("trusted redirect source should be approved");
    let redirect_response = redirect_approval
        .http_client()
        .get(redirect_source.endpoint())
        .send()
        .await
        .expect("redirect source should return its response");

    assert_eq!(redirect_response.status(), reqwest::StatusCode::FOUND);
    assert_eq!(redirect_source.request_count(), 1);
    assert_eq!(redirect_target.request_count(), 0);
}

#[tokio::test]
async fn provider_file_response_limit_rejects_oversized_body() {
    let oversized_file_server = CaptureServer::spawn_success(
        "application/json",
        "x".repeat(super::network::PROVIDER_HTTP_MAX_RESPONSE_BODY_BYTES + 1),
    );
    let oversized_file_request = model_request(
        ProviderDriver::OpenAiCompatible,
        oversized_file_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "oversized file response probe",
        0,
    );
    let trusted_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
    let oversized_file_approval = trusted_provider
        .approve_endpoint(&oversized_file_request)
        .await
        .expect("trusted file endpoint should be approved");
    let file_upload_client = FileUploadClient::from_request(&oversized_file_request, &oversized_file_approval)
        .expect("file upload client should use the approved HTTP client");
    let file_attachment = ModelFileAttachment {
        name: "bounded.txt".to_string(),
        content: "bounded request".to_string(),
        purpose: "file-extract".to_string(),
    };
    let file_error = file_upload_client
        .upload(&file_attachment, "security-agent")
        .await
        .expect_err("oversized file response body should be rejected");

    assert_eq!(
        file_error.diagnostic().message,
        "provider response body exceeded the configured limit"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detached_file_cleanup_is_capacity_bounded_timed_and_secret_safe() {
    const CLEANUP_AGENT_SECRET: &str = "cleanup-agent-secret-sentinel";
    const CLEANUP_FILE_SECRET: &str = "cleanup-file-secret-sentinel";
    const EXCESS_CLEANUP_COUNT: usize = 32;

    let _environment_lock = ENVIRONMENT_LOCK.lock().await;

    CAPTURED_LOGGER.enable_and_clear();

    let listener = TcpListener::bind("127.0.0.1:0").expect("cleanup listener should bind");
    let endpoint = format!("http://{}", listener.local_addr().expect("cleanup listener address should exist"));
    let (request_received_sender, request_received_receiver) = std::sync::mpsc::channel();
    let server_thread = thread::spawn(move || {
        let (stream, _peer_address) = listener.accept().expect("cleanup request should connect");
        let request = read_http_request(&stream).expect("cleanup request should parse");

        request_received_sender
            .send(request)
            .expect("cleanup request arrival should be observable");
        thread::sleep(Duration::from_millis(500));
    });
    let request = model_request(
        ProviderDriver::OpenAiCompatible,
        endpoint,
        Some(EXPLICIT_API_KEY.to_string()),
        HOSTILE_PROMPT_SECRET,
        0,
    );
    let trusted_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
    let endpoint_approval = trusted_provider
        .approve_endpoint(&request)
        .await
        .expect("trusted cleanup endpoint should be approved");
    let file_upload_client =
        FileUploadClient::from_request(&request, &endpoint_approval).expect("cleanup client should use the approved endpoint");
    let cleanup_executor = ProviderFileCleanupExecutor::with_limits(1, Duration::from_millis(100));
    let cleanup_command = |uploaded_file_id: String| DetachedProviderFileCleanup {
        file_upload_client: file_upload_client.clone(),
        agent_name: CLEANUP_AGENT_SECRET.to_string(),
        uploaded_file_ids: vec![uploaded_file_id],
    };

    assert_eq!(
        cleanup_executor.schedule(cleanup_command(CLEANUP_FILE_SECRET.to_string())),
        ProviderFileCleanupScheduleOutcome::Scheduled
    );

    let captured_request = request_received_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("first cleanup request should start");

    assert!(captured_request.head.starts_with("DELETE "));

    for cleanup_index in 0..EXCESS_CLEANUP_COUNT {
        assert_eq!(
            cleanup_executor.schedule(cleanup_command(format!("{CLEANUP_FILE_SECRET}-{cleanup_index}"))),
            ProviderFileCleanupScheduleOutcome::AtCapacity
        );
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        while cleanup_executor.available_permits() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("timed-out cleanup should release its only permit");

    server_thread.join().expect("cleanup server thread should stop");

    let captured_logs = CAPTURED_LOGGER.contents();

    for secret_value in [CLEANUP_AGENT_SECRET, CLEANUP_FILE_SECRET, EXPLICIT_API_KEY, HOSTILE_PROMPT_SECRET] {
        assert!(!captured_logs.contains(secret_value), "cleanup logs leaked `{secret_value}`");
    }
}

#[tokio::test]
async fn provider_stream_limits_reject_oversized_and_truncated_content() {
    let oversized_sse_server = CaptureServer::spawn_success(
        "text/event-stream",
        format!("data: {}", "x".repeat(cersei_provider::MAX_PROVIDER_SSE_FRAME_BYTES + 1)),
    );
    let oversized_sse_request = model_request(
        ProviderDriver::OpenAiCompatible,
        oversized_sse_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "oversized SSE probe",
        0,
    );
    let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);
    let sse_error = model_provider
        .generate(oversized_sse_request)
        .await
        .expect_err("oversized SSE frame should be rejected");

    assert_eq!(sse_error.diagnostic().message, "provider request failed");
    assert_eq!(oversized_sse_server.request_count(), 1);

    let truncated_sse_server = CaptureServer::spawn_success("text/event-stream", "data: {}".to_string());
    let truncated_sse_request = model_request(
        ProviderDriver::OpenAiCompatible,
        truncated_sse_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "truncated SSE probe",
        0,
    );
    let truncated_error = model_provider
        .generate(truncated_sse_request)
        .await
        .expect_err("SSE stream without a completion event should fail deterministically");

    assert_eq!(truncated_error.diagnostic().message, "provider request failed");
    assert_eq!(truncated_sse_server.request_count(), 1);

    let tool_argument_chunk = "x".repeat(cersei_provider::MAX_PROVIDER_TOOL_ARGUMENT_BYTES / 4 + 1);
    let mut tool_argument_stream = String::new();

    for chunk_index in 0..4 {
        let chunk = json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": (chunk_index == 0).then_some("call-1"),
                        "function": {
                            "name": (chunk_index == 0).then_some("finalize"),
                            "arguments": tool_argument_chunk,
                        }
                    }]
                },
                "finish_reason": null
            }]
        });

        writeln!(&mut tool_argument_stream, "data: {chunk}").expect("writing to a String should not fail");
    }

    tool_argument_stream.push_str("data: [DONE]\n");

    let oversized_tool_server = CaptureServer::spawn_success("text/event-stream", tool_argument_stream);
    let oversized_tool_request = model_request(
        ProviderDriver::OpenAiCompatible,
        oversized_tool_server.endpoint().to_string(),
        Some(EXPLICIT_API_KEY.to_string()),
        "oversized tool arguments probe",
        0,
    );
    let tool_error = model_provider
        .generate(oversized_tool_request)
        .await
        .expect_err("oversized accumulated tool arguments should be rejected");

    assert_eq!(tool_error.diagnostic().message, "provider request failed");
    assert_eq!(oversized_tool_server.request_count(), 1);
}

impl DriverSecurityCase {
    async fn assert_credential_resolution_matrix(self, custom_endpoint: &str) {
        let default_endpoint = self
            .driver
            .default_endpoint()
            .expect("every supported driver should have a built-in endpoint");
        let expected_ambient_key = self.uses_ambient_api_key.then(|| AMBIENT_API_KEY.to_string());
        let implicit_default = ProviderConfig {
            driver: self.driver,
            endpoint: None,
            api_key: None,
        };
        let explicit_default = ProviderConfig {
            driver: self.driver,
            endpoint: Some(default_endpoint.to_string()),
            api_key: None,
        };
        let missing_custom = ProviderConfig {
            driver: self.driver,
            endpoint: Some(custom_endpoint.to_string()),
            api_key: None,
        };
        let explicit_default_key = ProviderConfig {
            driver: self.driver,
            endpoint: None,
            api_key: Some(EXPLICIT_API_KEY.to_string()),
        };
        let explicit_custom_key = ProviderConfig {
            driver: self.driver,
            endpoint: Some(custom_endpoint.to_string()),
            api_key: Some(EXPLICIT_API_KEY.to_string()),
        };

        assert_eq!(implicit_default.resolved_api_key(), expected_ambient_key);
        assert_eq!(explicit_default.resolved_api_key(), expected_ambient_key);
        assert_eq!(missing_custom.resolved_api_key(), None);
        assert_eq!(explicit_default_key.resolved_api_key().as_deref(), Some(EXPLICIT_API_KEY));
        assert_eq!(explicit_custom_key.resolved_api_key().as_deref(), Some(EXPLICIT_API_KEY));
        assert!(!implicit_default.has_custom_endpoint());
        assert!(!explicit_default.has_custom_endpoint());
        assert!(missing_custom.has_custom_endpoint());

        self.assert_provider_builds(
            default_endpoint,
            implicit_default,
            explicit_default,
            missing_custom,
            explicit_default_key,
            explicit_custom_key,
        )
        .await;
    }

    async fn assert_provider_builds(
        self,
        default_endpoint: &str,
        implicit_default: ProviderConfig,
        explicit_default: ProviderConfig,
        missing_custom: ProviderConfig,
        explicit_default_key: ProviderConfig,
        explicit_custom_key: ProviderConfig,
    ) {
        let built_in_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::BuiltInOnly);
        let trusted_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);

        let mut build_request = model_request(self.driver, default_endpoint.to_string(), None, "credential resolution probe", 0);
        build_request.provider_config = implicit_default;
        let implicit_default_approval = built_in_provider
            .approve_endpoint(&build_request)
            .await
            .expect("built-in endpoint should be approved");
        let implicit_default_build = build_request
            .provider_config
            .build_provider(&build_request, &implicit_default_approval);
        let default_missing_key_is_valid = self.uses_ambient_api_key || self.permits_missing_api_key;

        assert_eq!(
            implicit_default_build.is_ok(),
            default_missing_key_is_valid,
            "{} default endpoint handled a missing key incorrectly",
            self.driver.as_str()
        );

        build_request.provider_config = explicit_default;
        let explicit_default_approval = built_in_provider
            .approve_endpoint(&build_request)
            .await
            .expect("exact built-in endpoint should be approved");

        assert_eq!(
            build_request
                .provider_config
                .build_provider(&build_request, &explicit_default_approval)
                .is_ok(),
            default_missing_key_is_valid,
            "{} exact built-in endpoint handled a missing key incorrectly",
            self.driver.as_str()
        );

        build_request.provider_config = explicit_default_key;
        let explicit_default_key_approval = built_in_provider
            .approve_endpoint(&build_request)
            .await
            .expect("built-in endpoint with explicit key should be approved");

        assert!(
            build_request
                .provider_config
                .build_provider(&build_request, &explicit_default_key_approval)
                .is_ok(),
            "{} rejected an explicit key for its default endpoint",
            self.driver.as_str()
        );

        build_request.provider_config = missing_custom;
        let missing_custom_approval = trusted_provider
            .approve_endpoint(&build_request)
            .await
            .expect("trusted custom endpoint should be approved");

        assert_eq!(
            build_request
                .provider_config
                .build_provider(&build_request, &missing_custom_approval)
                .is_ok(),
            self.permits_missing_api_key,
            "{} custom endpoint handled a missing explicit key incorrectly",
            self.driver.as_str()
        );

        build_request.provider_config = explicit_custom_key;
        let explicit_custom_key_approval = trusted_provider
            .approve_endpoint(&build_request)
            .await
            .expect("trusted custom endpoint with explicit key should be approved");

        assert!(
            build_request
                .provider_config
                .build_provider(&build_request, &explicit_custom_key_approval)
                .is_ok(),
            "{} rejected an explicit key for a custom endpoint",
            self.driver.as_str()
        );
    }
}

fn assert_request_excludes_secret(request: &CapturedRequest, secret: &str) {
    assert!(!request.head.contains(secret), "captured request leaked ambient credential");
    assert!(!request.body.contains(secret), "captured request body leaked ambient credential");
}

fn assert_file_failure(diagnostic: &superwire_protocol::event::ExecutorDiagnostic, operation: FileProviderOperation) {
    let serialized_diagnostic = serde_json::to_string(diagnostic).expect("file diagnostic should serialize");
    let ExecutorDiagnosticSubject::Provider { http_status, .. } = &diagnostic.subject else {
        panic!("file failure should retain provider metadata");
    };

    assert_eq!(diagnostic.message, operation.failure_message());
    assert_eq!(*http_status, Some(500));

    for forbidden_value in [HOSTILE_BODY_SECRET, HOSTILE_PROMPT_SECRET, EXPLICIT_API_KEY] {
        assert!(!serialized_diagnostic.contains(forbidden_value));
    }
}

fn model_request(driver: ProviderDriver, endpoint: String, api_key: Option<String>, prompt: &str, max_retries: u32) -> ModelRequest {
    ModelRequest {
        agent_name: "security-agent".to_string(),
        provider_config: ProviderConfig {
            driver,
            endpoint: Some(endpoint),
            api_key,
        },
        model_name: "security-model".to_string(),
        wire_api: ModelWireApi::ChatCompletion,
        inference: [
            (InferenceParameter::ProviderMaxRetries.as_str().to_string(), json!(max_retries)),
            (InferenceParameter::ProviderRetryBaseDelayMs.as_str().to_string(), json!(1)),
        ]
        .into_iter()
        .collect(),
        context: None,
        prompt: prompt.to_string(),
        prompt_content: Vec::new(),
        file_attachments: Vec::new(),
        output_schema: ModelSchema::OpenObject,
        tools: vec![ModelToolDefinition::finalize(ModelSchema::OpenObject)],
        event_sender: None,
        mcp_pool: McpClientPool::empty(),
        tool_call_tracker: ToolCallTracker::default(),
    }
}

struct EnvironmentGuard {
    original_values: Vec<(&'static str, Option<OsString>)>,
}

impl EnvironmentGuard {
    fn set_all(value: &str) -> Self {
        let original_values = ENVIRONMENT_VARIABLES
            .into_iter()
            .map(|variable_name| {
                let original_value = std::env::var_os(variable_name);

                std::env::set_var(variable_name, value);

                (variable_name, original_value)
            })
            .collect();

        Self { original_values }
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        for (variable_name, original_value) in &self.original_values {
            match original_value {
                Some(value) => std::env::set_var(variable_name, value),
                None => std::env::remove_var(variable_name),
            }
        }
    }
}

struct CapturedLogger {
    records: Mutex<Vec<String>>,
}

impl CapturedLogger {
    fn enable_and_clear(&'static self) {
        LOG_INITIALIZATION.call_once(|| {
            log::set_logger(self).expect("security test logger should install");
            log::set_max_level(LevelFilter::Debug);
        });
        self.records.lock().expect("log records lock should not be poisoned").clear();
    }

    fn contents(&self) -> String {
        self.records.lock().expect("log records lock should not be poisoned").join("\n")
    }
}

impl Log for CapturedLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.records
                .lock()
                .expect("log records lock should not be poisoned")
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

#[derive(Clone)]
struct CapturedRequest {
    head: String,
    body: String,
}

impl CapturedRequest {
    fn contains(&self, value: &str) -> bool {
        self.head.contains(value) || self.body.contains(value)
    }
}

#[derive(Clone)]
struct CaptureResponse {
    status: &'static str,
    content_type: &'static str,
    headers: Vec<(String, String)>,
    body: String,
}

impl CaptureResponse {
    fn error(body: String) -> Self {
        Self {
            status: "500 Internal Server Error",
            content_type: "application/json",
            headers: Vec::new(),
            body,
        }
    }

    fn redirect(location: String) -> Self {
        Self {
            status: "302 Found",
            content_type: "text/plain",
            headers: vec![("Location".to_string(), location)],
            body: String::new(),
        }
    }

    fn success(content_type: &'static str, body: String) -> Self {
        Self {
            status: "200 OK",
            content_type,
            headers: Vec::new(),
            body,
        }
    }

    fn write_to(&self, stream: &mut TcpStream) {
        let mut additional_headers = String::new();

        for (header_name, header_value) in &self.headers {
            additional_headers.push_str(header_name);
            additional_headers.push_str(": ");
            additional_headers.push_str(header_value);
            additional_headers.push_str("\r\n");
        }

        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.content_type,
            additional_headers,
            self.body.len(),
            self.body
        );

        if let Err(error) = stream.write_all(response.as_bytes()) {
            assert!(
                matches!(error.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset),
                "capture response should write: {error}"
            );
        }
    }
}

struct CaptureServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown: Arc<AtomicBool>,
    server_thread: Option<JoinHandle<()>>,
}

impl CaptureServer {
    fn spawn(response_body: String) -> Self {
        Self::spawn_response(CaptureResponse::error(response_body))
    }

    fn spawn_redirect(location: String) -> Self {
        Self::spawn_response(CaptureResponse::redirect(location))
    }

    fn spawn_success(content_type: &'static str, body: String) -> Self {
        Self::spawn_response(CaptureResponse::success(content_type, body))
    }

    fn spawn_response(response: CaptureResponse) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("capture listener should bind");
        listener.set_nonblocking(true).expect("capture listener should become nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().expect("capture address should exist"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let server_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _peer_address)) => {
                        let request = read_http_request(&stream).expect("provider request should parse");

                        thread_requests
                            .lock()
                            .expect("captured requests lock should not be poisoned")
                            .push(request);
                        response.write_to(&mut stream);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("capture listener failed: {error}"),
                }
            }
        });

        Self {
            endpoint,
            requests,
            shutdown,
            server_thread: Some(server_thread),
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("captured requests lock should not be poisoned").len()
    }

    fn latest_request(&self) -> CapturedRequest {
        self.requests
            .lock()
            .expect("captured requests lock should not be poisoned")
            .last()
            .expect("capture server should have a request")
            .clone()
    }
}

impl Drop for CaptureServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);

        if let Some(server_thread) = self.server_thread.take() {
            server_thread.join().expect("capture server thread should stop");
        }
    }
}

fn read_http_request(stream: &TcpStream) -> Option<CapturedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;

    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_head = String::new();
    let mut request_line = String::new();

    reader.read_line(&mut request_line).ok()?;
    request_head.push_str(&request_line);

    let mut content_length = 0_usize;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        reader.read_line(&mut header_line).ok()?;
        request_head.push_str(&header_line);

        if header_line == "\r\n" || header_line.is_empty() {
            break;
        }

        if let Some((header_name, header_value)) = header_line.trim_end().split_once(':') {
            if header_name.eq_ignore_ascii_case("content-length") {
                content_length = header_value.trim().parse().ok()?;
            }
        }
    }

    let mut body_bytes = vec![0_u8; content_length];
    reader.read_exact(&mut body_bytes).ok()?;

    Some(CapturedRequest {
        head: request_head,
        body: String::from_utf8_lossy(&body_bytes).into_owned(),
    })
}
