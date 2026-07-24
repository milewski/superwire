use crate::McpError;
use std::fmt::{Debug, Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::time::{Duration, Instant};
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use url::{Host, Url};

pub const MCP_HTTP_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
pub const MCP_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const MCP_HTTP_SEND_TIMEOUT: Duration = Duration::from_secs(5);
pub const MCP_HTTP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub const MCP_HTTP_BODY_TIMEOUT: Duration = Duration::from_secs(10);
pub const MCP_HTTP_GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);
pub const MCP_HTTP_MAX_RESPONSE_BODY_BYTES: u64 = 4 * 1024 * 1024;
pub const MCP_ENDPOINT_APPROVAL_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum McpNetworkPolicy {
    #[default]
    Disabled,
    PublicOnly,
    Trusted,
}

impl McpNetworkPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::PublicOnly => "public-only",
            Self::Trusted => "trusted",
        }
    }

    pub(crate) fn approve_endpoint(
        self,
        server_name: &str,
        endpoint: &str,
        dns_resolver: &dyn McpDnsResolver,
    ) -> Result<McpEndpointApproval, McpError> {
        if self == Self::Disabled {
            return Err(self.denied(server_name, "outbound MCP networking is disabled"));
        }

        let parsed_endpoint = McpHttpEndpoint::parse(endpoint).map_err(|message| self.denied(server_name, message))?;
        let approved_transport = if self == Self::Trusted {
            McpApprovedTransport::Trusted
        } else {
            parsed_endpoint.approve_public_transport(server_name, self, dns_resolver)?
        };

        Ok(McpEndpointApproval {
            server_name: server_name.to_string(),
            endpoint: endpoint.to_string(),
            approved_transport,
            approved_at: Instant::now(),
        })
    }

    fn denied(self, server_name: &str, message: impl Into<String>) -> McpError {
        McpError::NetworkPolicyViolation {
            server_name: server_name.to_string(),
            policy: self,
            message: message.into(),
        }
    }
}

impl Display for McpNetworkPolicy {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for McpNetworkPolicy {
    type Err = McpNetworkPolicyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "disabled" => Ok(Self::Disabled),
            "public-only" => Ok(Self::PublicOnly),
            "trusted" => Ok(Self::Trusted),
            _ => Err(McpNetworkPolicyParseError { value: value.to_string() }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid MCP network policy `{value}`; expected disabled, public-only, or trusted")]
pub struct McpNetworkPolicyParseError {
    value: String,
}

pub trait McpDnsResolver: Debug + Send + Sync {
    fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemMcpDnsResolver;

impl McpDnsResolver for SystemMcpDnsResolver {
    fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        (hostname, port).to_socket_addrs().map(Iterator::collect)
    }
}
#[derive(Debug, Clone)]
pub struct McpEndpointApproval {
    server_name: String,
    endpoint: String,
    approved_transport: McpApprovedTransport,
    approved_at: Instant,
}

#[derive(Debug, Clone)]
enum McpApprovedTransport {
    Public { socket_addresses: Vec<SocketAddr> },
    Trusted,
    Unrestricted,
}

impl McpEndpointApproval {
    pub(crate) fn unrestricted(server_name: &str, endpoint: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
            endpoint: endpoint.to_string(),
            approved_transport: McpApprovedTransport::Unrestricted,
            approved_at: Instant::now(),
        }
    }

    pub(crate) fn validate_for_dispatch(&self, server_config: &crate::McpServerConfig) -> Result<(), McpError> {
        if self.server_name != server_config.name || self.endpoint != server_config.endpoint {
            return Err(McpError::EndpointApprovalMismatch {
                server_name: server_config.name.clone(),
            });
        }

        if self.approved_at.elapsed() > MCP_ENDPOINT_APPROVAL_TTL {
            return Err(McpError::EndpointApprovalExpired {
                server_name: server_config.name.clone(),
            });
        }

        Ok(())
    }

    pub(crate) fn is_policy_approved(&self) -> bool {
        !matches!(self.approved_transport, McpApprovedTransport::Unrestricted)
    }

    #[cfg(test)]
    pub(crate) fn expire_for_test(&mut self) {
        self.approved_at = Instant::now()
            .checked_sub(MCP_ENDPOINT_APPROVAL_TTL + Duration::from_millis(1))
            .expect("MCP approval expiry offset should fit");
    }

    #[cfg(test)]
    pub(crate) fn http_agent_with_timeout(&self, timeout: Duration) -> ureq::Agent {
        self.http_agent_with_bounds(McpHttpBounds {
            resolve: timeout,
            connect: timeout,
            send: timeout,
            response: timeout,
            body: timeout,
            global: timeout,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct McpHttpBounds {
    resolve: Duration,
    connect: Duration,
    send: Duration,
    response: Duration,
    body: Duration,
    global: Duration,
}

impl McpHttpBounds {
    const DEFAULT: Self = Self {
        resolve: MCP_HTTP_RESOLVE_TIMEOUT,
        connect: MCP_HTTP_CONNECT_TIMEOUT,
        send: MCP_HTTP_SEND_TIMEOUT,
        response: MCP_HTTP_RESPONSE_TIMEOUT,
        body: MCP_HTTP_BODY_TIMEOUT,
        global: MCP_HTTP_GLOBAL_TIMEOUT,
    };

    fn agent_config(self) -> Config {
        Config::builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_global(Some(self.global))
            .timeout_per_call(Some(self.global))
            .timeout_resolve(Some(self.resolve))
            .timeout_connect(Some(self.connect))
            .timeout_send_request(Some(self.send))
            .timeout_send_body(Some(self.send))
            .timeout_recv_response(Some(self.response))
            .timeout_recv_body(Some(self.body))
            .build()
    }
}

impl McpEndpointApproval {
    pub(crate) fn http_agent(&self) -> ureq::Agent {
        self.http_agent_with_bounds(McpHttpBounds::DEFAULT)
    }

    fn http_agent_with_bounds(&self, http_bounds: McpHttpBounds) -> ureq::Agent {
        let config = http_bounds.agent_config();
        match &self.approved_transport {
            McpApprovedTransport::Public { socket_addresses } => ureq::Agent::with_parts(
                config,
                DefaultConnector::default(),
                PinnedMcpResolver {
                    socket_addresses: socket_addresses.clone(),
                },
            ),
            McpApprovedTransport::Trusted | McpApprovedTransport::Unrestricted => config.into(),
        }
    }
}

#[derive(Debug)]
struct PinnedMcpResolver {
    socket_addresses: Vec<SocketAddr>,
}

impl Resolver for PinnedMcpResolver {
    fn resolve(&self, _uri: &Uri, _config: &Config, _timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut resolved_addresses = self.empty();

        for socket_address in &self.socket_addresses {
            resolved_addresses.push(*socket_address);
        }

        if resolved_addresses.is_empty() {
            return Err(ureq::Error::HostNotFound);
        }

        Ok(resolved_addresses)
    }
}

#[derive(Debug, Clone, Copy)]
enum McpHttpScheme {
    Http,
    Https,
}

impl McpHttpScheme {
    fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            _ => Err("endpoint scheme must be http or https; local process transports are not enabled"),
        }
    }

    const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

#[derive(Debug)]
struct McpHttpEndpoint {
    host: Host<String>,
    port: u16,
}

impl McpHttpEndpoint {
    fn parse(endpoint: &str) -> Result<Self, &'static str> {
        let endpoint_url = Url::parse(endpoint).map_err(|_error| "endpoint must be a valid absolute URL")?;
        let scheme = McpHttpScheme::parse(endpoint_url.scheme())?;

        if !endpoint_url.username().is_empty() || endpoint_url.password().is_some() {
            return Err("endpoint URL must not contain user information");
        }

        let host = endpoint_url.host().ok_or("endpoint URL must include a host")?.to_owned();
        let port = endpoint_url.port().unwrap_or_else(|| scheme.default_port());

        Ok(Self { host, port })
    }

    fn approve_public_transport(
        &self,
        server_name: &str,
        network_policy: McpNetworkPolicy,
        dns_resolver: &dyn McpDnsResolver,
    ) -> Result<McpApprovedTransport, McpError> {
        let socket_addresses = match &self.host {
            Host::Ipv4(ipv4_address) => vec![SocketAddr::new(IpAddr::V4(*ipv4_address), self.port)],
            Host::Ipv6(ipv6_address) => vec![SocketAddr::new(IpAddr::V6(*ipv6_address), self.port)],
            Host::Domain(hostname) => {
                if Self::is_forbidden_hostname(hostname) {
                    return Err(network_policy.denied(server_name, "endpoint hostname is reserved for local or metadata access"));
                }

                dns_resolver
                    .resolve(hostname, self.port)
                    .map_err(|_error| network_policy.denied(server_name, "endpoint hostname could not be resolved safely"))?
            }
        };
        let socket_addresses = socket_addresses
            .into_iter()
            .map(|socket_address| SocketAddr::new(socket_address.ip(), self.port))
            .collect::<Vec<_>>();

        if socket_addresses.is_empty() {
            return Err(network_policy.denied(server_name, "endpoint hostname did not resolve to any address"));
        }

        if socket_addresses
            .iter()
            .any(|socket_address| McpEndpointAddress::new(socket_address.ip()).is_forbidden())
        {
            return Err(network_policy.denied(server_name, "endpoint resolves to a non-public address"));
        }

        let mut pinned_addresses = socket_addresses;
        pinned_addresses.sort_unstable();
        pinned_addresses.dedup();

        Ok(McpApprovedTransport::Public {
            socket_addresses: pinned_addresses,
        })
    }

    fn is_forbidden_hostname(hostname: &str) -> bool {
        let normalized_hostname = hostname.trim_end_matches('.').to_ascii_lowercase();

        let final_hostname_label = normalized_hostname.rsplit('.').next().unwrap_or_default();

        matches!(final_hostname_label, "localhost" | "local" | "internal" | "home" | "lan")
            || normalized_hostname == "metadata"
            || normalized_hostname.starts_with("metadata.")
    }
}

#[derive(Debug, Clone, Copy)]
struct McpEndpointAddress(IpAddr);

impl McpEndpointAddress {
    const CLOUD_METADATA_IPV4: [Ipv4Addr; 2] = [Ipv4Addr::new(168, 63, 129, 16), Ipv4Addr::new(100, 100, 100, 200)];

    const fn new(ip_address: IpAddr) -> Self {
        Self(ip_address)
    }

    fn is_forbidden(self) -> bool {
        match self.0 {
            IpAddr::V4(ipv4_address) => Self::is_forbidden_ipv4(ipv4_address),
            IpAddr::V6(ipv6_address) => Self::is_forbidden_ipv6(ipv6_address),
        }
    }

    fn is_forbidden_ipv4(ipv4_address: Ipv4Addr) -> bool {
        let octets = ipv4_address.octets();

        ipv4_address.is_unspecified()
            || ipv4_address.is_loopback()
            || ipv4_address.is_private()
            || ipv4_address.is_link_local()
            || ipv4_address.is_multicast()
            || ipv4_address == Ipv4Addr::BROADCAST
            || octets[0] == 0
            || (octets[0] == 100 && (64..=127).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
            || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
            || (octets[0] == 192 && octets[1] == 88 && octets[2] == 99)
            || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
            || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
            || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
            || octets[0] >= 240
            || Self::CLOUD_METADATA_IPV4.contains(&ipv4_address)
    }

    fn is_forbidden_ipv6(ipv6_address: Ipv6Addr) -> bool {
        if let Some(mapped_ipv4_address) = ipv6_address.to_ipv4_mapped() {
            return Self::is_forbidden_ipv4(mapped_ipv4_address);
        }

        let segments = ipv6_address.segments();
        let is_global_unicast = segments[0] & 0xe000 == 0x2000;
        let is_documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
        let is_additional_documentation = segments[0] == 0x3fff && segments[1] & 0xf000 == 0;
        let is_ietf_special = segments[0] == 0x2001 && segments[1] <= 0x01ff;
        let is_six_to_four = segments[0] == 0x2002;

        ipv6_address.is_unspecified()
            || ipv6_address.is_loopback()
            || ipv6_address.is_multicast()
            || !is_global_unicast
            || is_documentation
            || is_additional_documentation
            || is_ietf_special
            || is_six_to_four
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpMcpClientFactory, McpClientFactory, McpClientRequestScope, McpServerConfig, PolicyMcpClientFactory};
    use std::collections::{BTreeMap, VecDeque};
    use std::io::ErrorKind;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct SequenceDnsResolver {
        responses: Mutex<VecDeque<Vec<IpAddr>>>,
    }

    impl McpDnsResolver for SequenceDnsResolver {
        fn resolve(&self, _hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
            let addresses = self
                .responses
                .lock()
                .expect("DNS response sequence lock should not be poisoned")
                .pop_front()
                .expect("test should configure enough DNS responses");

            Ok(addresses.into_iter().map(|ip_address| SocketAddr::new(ip_address, port)).collect())
        }
    }

    #[derive(Debug, Default)]
    struct TestDnsResolver {
        addresses_by_hostname: BTreeMap<String, Vec<IpAddr>>,
        requested_hostnames: Arc<Mutex<Vec<String>>>,
    }

    impl TestDnsResolver {
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

    impl McpDnsResolver for TestDnsResolver {
        fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
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

    #[test]
    fn disabled_rejects_every_endpoint_without_dns() {
        let dns_resolver = TestDnsResolver::with_host("example.com", [IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))]);
        let error = McpNetworkPolicy::Disabled
            .approve_endpoint("local", "https://example.com/mcp", &dns_resolver)
            .expect_err("disabled policy should reject outbound MCP");

        assert!(matches!(error, McpError::NetworkPolicyViolation { .. }));
        assert_eq!(dns_resolver.request_count(), 0);
    }

    #[test]
    fn default_http_factory_rejects_loopback_before_opening_a_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        listener.set_nonblocking(true).expect("test listener should become nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().expect("test listener address should exist"));
        let error = HttpMcpClientFactory
            .client_for_config(McpServerConfig {
                name: "local".to_string(),
                endpoint,
                headers: BTreeMap::new(),
            })
            .expect_err("default HTTP factory should disable outbound MCP");

        assert!(error.is_network_policy_violation());
        assert_eq!(
            listener.accept().expect_err("disabled policy must not open a socket").kind(),
            ErrorKind::WouldBlock
        );
    }

    #[test]
    fn public_only_accepts_public_ipv4_and_ipv6_literals() {
        let dns_resolver = TestDnsResolver::default();

        for endpoint in ["https://8.8.8.8/mcp", "https://[2606:4700:4700::1111]/mcp"] {
            McpNetworkPolicy::PublicOnly
                .approve_endpoint("public", endpoint, &dns_resolver)
                .expect("public address should be allowed");
        }

        assert_eq!(dns_resolver.request_count(), 0);
    }

    #[test]
    fn public_only_rejects_forbidden_ipv4_ranges() {
        let dns_resolver = TestDnsResolver::default();
        let forbidden_addresses = [
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
        ];

        for forbidden_address in forbidden_addresses {
            let endpoint = format!("http://{forbidden_address}/mcp");
            McpNetworkPolicy::PublicOnly
                .approve_endpoint("public", &endpoint, &dns_resolver)
                .expect_err("non-public IPv4 address should be rejected");
        }
    }

    #[test]
    fn public_only_rejects_forbidden_ipv6_ranges_and_mapped_ipv4() {
        let dns_resolver = TestDnsResolver::default();
        let forbidden_addresses = [
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
        ];

        for forbidden_address in forbidden_addresses {
            let endpoint = format!("http://[{forbidden_address}]/mcp");
            McpNetworkPolicy::PublicOnly
                .approve_endpoint("public", &endpoint, &dns_resolver)
                .expect_err("non-public IPv6 address should be rejected");
        }
    }

    #[test]
    fn public_only_rejects_userinfo_unsupported_schemes_and_reserved_hostnames_without_dns() {
        let dns_resolver = TestDnsResolver::default();
        let forbidden_endpoints = [
            "https://user:password@example.com/mcp",
            "file:///tmp/mcp.sock",
            "stdio://tool",
            "http://localhost/mcp",
            "http://service.local/mcp",
            "http://metadata.google.internal/computeMetadata/v1",
        ];

        for forbidden_endpoint in forbidden_endpoints {
            McpNetworkPolicy::PublicOnly
                .approve_endpoint("public", forbidden_endpoint, &dns_resolver)
                .expect_err("unsafe endpoint form should be rejected");
        }

        assert_eq!(dns_resolver.request_count(), 0);
    }

    #[test]
    fn public_only_resolves_every_hostname_and_rejects_mixed_public_private_answers() {
        let dns_resolver = TestDnsResolver::with_host(
            "example.com",
            [IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), IpAddr::V4(Ipv4Addr::LOCALHOST)],
        );

        McpNetworkPolicy::PublicOnly
            .approve_endpoint("public", "https://example.com/mcp", &dns_resolver)
            .expect_err("one forbidden DNS answer should reject the endpoint");
        assert_eq!(dns_resolver.request_count(), 1);
    }

    #[test]
    fn public_only_pins_all_vetted_public_dns_answers() {
        let expected_addresses = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 8443),
            SocketAddr::new(
                "2606:2800:220:1:248:1893:25c8:1946".parse().expect("public IPv6 should parse"),
                8443,
            ),
        ];
        let dns_resolver = TestDnsResolver::with_host("example.com", expected_addresses.iter().map(std::net::SocketAddr::ip));
        let approval = McpNetworkPolicy::PublicOnly
            .approve_endpoint("public", "https://example.com:8443/mcp", &dns_resolver)
            .expect("public hostname should be approved");
        let McpApprovedTransport::Public { socket_addresses } = approval.approved_transport else {
            panic!("expected public endpoint approval");
        };

        assert_eq!(socket_addresses, expected_addresses);
        assert_eq!(dns_resolver.request_count(), 1);
    }

    #[test]
    fn approved_http_agent_uses_every_mandatory_timeout() {
        let dns_resolver = TestDnsResolver::default();
        let endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint("local", "http://127.0.0.1:3000/mcp", &dns_resolver)
            .expect("trusted endpoint should be approved");
        let http_agent = endpoint_approval.http_agent();
        let timeouts = http_agent.config().timeouts();

        assert_eq!(timeouts.resolve, Some(MCP_HTTP_RESOLVE_TIMEOUT));
        assert_eq!(timeouts.connect, Some(MCP_HTTP_CONNECT_TIMEOUT));
        assert_eq!(timeouts.send_request, Some(MCP_HTTP_SEND_TIMEOUT));
        assert_eq!(timeouts.send_body, Some(MCP_HTTP_SEND_TIMEOUT));
        assert_eq!(timeouts.recv_response, Some(MCP_HTTP_RESPONSE_TIMEOUT));
        assert!(http_agent.config().proxy().is_none());
        assert_eq!(http_agent.config().max_redirects(), 0);
        assert_eq!(timeouts.recv_body, Some(MCP_HTTP_BODY_TIMEOUT));
        assert_eq!(timeouts.global, Some(MCP_HTTP_GLOBAL_TIMEOUT));
    }

    #[test]
    fn expired_endpoint_approval_cannot_create_a_client() {
        let dns_resolver = TestDnsResolver::default();
        let mut endpoint_approval = McpNetworkPolicy::Trusted
            .approve_endpoint("local", "http://127.0.0.1:3000/mcp", &dns_resolver)
            .expect("trusted endpoint should be approved");
        endpoint_approval.expire_for_test();
        let server_config = McpServerConfig {
            name: "local".to_string(),
            endpoint: "http://127.0.0.1:3000/mcp".to_string(),
            headers: BTreeMap::new(),
        };

        assert!(matches!(
            endpoint_approval.validate_for_dispatch(&server_config),
            Err(McpError::EndpointApprovalExpired { .. })
        ));
    }

    #[test]
    fn request_scope_churn_reapproves_dns_and_does_not_reuse_stale_addresses() {
        let first_address = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
        let second_address = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        let dns_resolver = Arc::new(SequenceDnsResolver {
            responses: Mutex::new(VecDeque::from([vec![first_address], vec![second_address]])),
        });
        let client_factory =
            PolicyMcpClientFactory::with_dns_resolver(McpNetworkPolicy::PublicOnly, Arc::clone(&dns_resolver) as Arc<dyn McpDnsResolver>);
        let server_configs = vec![McpServerConfig {
            name: "public".to_string(),
            endpoint: "https://example.com/mcp".to_string(),
            headers: BTreeMap::new(),
        }];
        let first_scope = McpClientRequestScope::from_server_configs(&client_factory, &server_configs).expect("first scope should approve");
        let first_approval = first_scope
            .approve_endpoint("public", "https://example.com/mcp")
            .expect("first scoped approval should exist");
        drop(first_scope);
        let second_scope =
            McpClientRequestScope::from_server_configs(&client_factory, &server_configs).expect("second scope should approve");
        let second_approval = second_scope
            .approve_endpoint("public", "https://example.com/mcp")
            .expect("second scoped approval should exist");

        let McpApprovedTransport::Public {
            socket_addresses: first_socket_addresses,
        } = first_approval.approved_transport
        else {
            panic!("first endpoint should use pinned public addresses");
        };
        let McpApprovedTransport::Public {
            socket_addresses: second_socket_addresses,
        } = second_approval.approved_transport
        else {
            panic!("second endpoint should use pinned public addresses");
        };

        assert_eq!(first_socket_addresses, vec![SocketAddr::new(first_address, 443)]);
        assert_eq!(second_socket_addresses, vec![SocketAddr::new(second_address, 443)]);
    }

    #[test]
    fn trusted_allows_loopback_without_dns_but_still_requires_http_transport() {
        let dns_resolver = TestDnsResolver::default();

        McpNetworkPolicy::Trusted
            .approve_endpoint("local", "http://127.0.0.1:3000/mcp", &dns_resolver)
            .expect("trusted policy should allow local HTTP endpoint");
        McpNetworkPolicy::Trusted
            .approve_endpoint("local", "stdio://tool", &dns_resolver)
            .expect_err("unsupported local process transport should remain rejected");
        assert_eq!(dns_resolver.request_count(), 0);
    }
}
