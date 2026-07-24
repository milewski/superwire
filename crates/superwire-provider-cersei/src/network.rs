use async_trait::async_trait;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use crate::ProviderConfigCerseiExt;
use reqwest::{Client, Url};
use superwire_model::{ModelProviderError as ProviderError, ModelRequest};
use superwire_semantic::support::provider::ProviderConfig;

pub const PROVIDER_HTTP_RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

pub const PROVIDER_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const PROVIDER_HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
pub const PROVIDER_HTTP_MAX_RESPONSE_BODY_BYTES: usize = 4 * 1024 * 1024;

#[async_trait]
pub(crate) trait ProviderDnsResolver: std::fmt::Debug + Send + Sync {
    async fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemProviderDnsResolver;

#[async_trait]
impl ProviderDnsResolver for SystemProviderDnsResolver {
    async fn resolve(&self, hostname: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        tokio::net::lookup_host((hostname, port)).await.map(Iterator::collect)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ProviderNetworkPolicy {
    #[default]
    BuiltInOnly,
    PublicOnly,
    Trusted,
}

impl ProviderNetworkPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltInOnly => "built-in-only",
            Self::PublicOnly => "public-only",
            Self::Trusted => "trusted",
        }
    }

    pub(crate) async fn approve(
        self,
        provider_config: &ProviderConfig,
        request: &ModelRequest,
        dns_resolver: &dyn ProviderDnsResolver,
    ) -> Result<ProviderEndpointApproval, ProviderError> {
        let endpoint = provider_config
            .resolved_endpoint()
            .ok_or_else(|| Self::rejected(request, "provider endpoint is not configured"))?;

        if !provider_config.has_custom_endpoint() {
            return ProviderEndpointApproval::built_in(&endpoint).map_err(|error| {
                ProviderError::model_with_source(request.agent_name.clone(), "failed to configure provider HTTP client", error)
            });
        }

        if self == Self::BuiltInOnly {
            return Err(Self::rejected(request, "custom provider endpoints are disabled"));
        }

        let parsed_endpoint = ProviderHttpEndpoint::parse(&endpoint).map_err(|message| Self::rejected(request, message))?;

        if self == Self::PublicOnly && parsed_endpoint.scheme != ProviderHttpScheme::Https {
            return Err(Self::rejected(request, "public-only provider endpoints must use HTTPS"));
        }

        if self == Self::PublicOnly && ProviderHttpEndpoint::is_forbidden_hostname(&parsed_endpoint.hostname) {
            return Err(Self::rejected(
                request,
                "provider endpoint hostname is reserved for local or metadata access",
            ));
        }

        let socket_addresses = parsed_endpoint
            .resolve(dns_resolver)
            .await
            .map_err(|message| Self::rejected(request, message))?;

        if self == Self::PublicOnly
            && socket_addresses
                .iter()
                .any(|socket_address| ProviderEndpointAddress::new(socket_address.ip()).is_forbidden())
        {
            return Err(Self::rejected(request, "provider endpoint resolves to a non-public address"));
        }

        ProviderEndpointApproval::custom(&endpoint, &parsed_endpoint.hostname, &socket_addresses).map_err(|error| {
            ProviderError::model_with_source(request.agent_name.clone(), "failed to configure provider HTTP client", error)
        })
    }

    fn rejected(request: &ModelRequest, message: impl Into<String>) -> ProviderError {
        ProviderError::model(request.agent_name.clone(), message.into())
    }
}

impl Display for ProviderNetworkPolicy {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ProviderNetworkPolicy {
    type Err = ProviderNetworkPolicyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "built-in-only" => Ok(Self::BuiltInOnly),
            "public-only" => Ok(Self::PublicOnly),
            "trusted" => Ok(Self::Trusted),
            _ => Err(ProviderNetworkPolicyParseError { value: value.to_string() }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid provider network policy `{value}`; expected built-in-only, public-only, or trusted")]
pub struct ProviderNetworkPolicyParseError {
    value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderEndpointApproval {
    endpoint: String,
    client: Client,
}

impl ProviderEndpointApproval {
    fn built_in(endpoint: &str) -> Result<Self, reqwest::Error> {
        Self::new(endpoint, None, &[])
    }

    fn custom(endpoint: &str, hostname: &str, socket_addresses: &[SocketAddr]) -> Result<Self, reqwest::Error> {
        let resolver_hostname = hostname.trim_matches(['[', ']']).parse::<IpAddr>().is_err().then_some(hostname);

        Self::new(endpoint, resolver_hostname, socket_addresses)
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn http_client(&self) -> Client {
        self.client.clone()
    }

    fn new(endpoint: &str, hostname: Option<&str>, socket_addresses: &[SocketAddr]) -> Result<Self, reqwest::Error> {
        let mut client_builder = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(PROVIDER_HTTP_CONNECT_TIMEOUT)
            .timeout(PROVIDER_HTTP_REQUEST_TIMEOUT);

        if let Some(hostname) = hostname {
            client_builder = client_builder.resolve_to_addrs(hostname, socket_addresses);
        }

        Ok(Self {
            endpoint: endpoint.to_string(),
            client: client_builder.build()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderHttpScheme {
    Http,
    Https,
}

impl ProviderHttpScheme {
    fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            _ => Err("provider endpoint scheme must be HTTP or HTTPS"),
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
struct ProviderHttpEndpoint {
    scheme: ProviderHttpScheme,
    hostname: String,
    port: u16,
}

impl ProviderHttpEndpoint {
    fn parse(endpoint: &str) -> Result<Self, &'static str> {
        let endpoint_url = Url::parse(endpoint).map_err(|_error| "provider endpoint must be a valid absolute URL")?;
        let scheme = ProviderHttpScheme::parse(endpoint_url.scheme())?;

        if !endpoint_url.username().is_empty() || endpoint_url.password().is_some() {
            return Err("provider endpoint URL must not contain user information");
        }

        if endpoint_url.query().is_some() || endpoint_url.fragment().is_some() {
            return Err("provider endpoint URL must not contain a query or fragment");
        }

        let hostname = endpoint_url
            .host_str()
            .ok_or("provider endpoint URL must include a host")?
            .to_ascii_lowercase();
        let port = endpoint_url.port().unwrap_or_else(|| scheme.default_port());

        if hostname.is_empty() {
            return Err("provider endpoint URL must include a host");
        }

        Ok(Self { scheme, hostname, port })
    }

    async fn resolve(&self, dns_resolver: &dyn ProviderDnsResolver) -> Result<Vec<SocketAddr>, &'static str> {
        if let Ok(ip_address) = self.hostname.trim_matches(['[', ']']).parse::<IpAddr>() {
            return Ok(vec![SocketAddr::new(ip_address, self.port)]);
        }

        let mut socket_addresses = tokio::time::timeout(PROVIDER_HTTP_RESOLVE_TIMEOUT, dns_resolver.resolve(&self.hostname, self.port))
            .await
            .map_err(|_error| "provider endpoint hostname resolution timed out")?
            .map_err(|_error| "provider endpoint hostname could not be resolved safely")?
            .into_iter()
            .map(|socket_address| SocketAddr::new(socket_address.ip(), self.port))
            .collect::<Vec<_>>();

        if socket_addresses.is_empty() {
            return Err("provider endpoint hostname did not resolve to any address");
        }

        socket_addresses.sort_unstable();
        socket_addresses.dedup();

        Ok(socket_addresses)
    }

    fn is_forbidden_hostname(hostname: &str) -> bool {
        let normalized_hostname = hostname.trim_end_matches('.');
        let final_hostname_label = normalized_hostname.rsplit('.').next().unwrap_or_default();

        matches!(final_hostname_label, "localhost" | "local" | "internal" | "home" | "lan")
            || normalized_hostname == "metadata"
            || normalized_hostname.starts_with("metadata.")
    }
}

#[derive(Debug, Clone, Copy)]
struct ProviderEndpointAddress(IpAddr);

impl ProviderEndpointAddress {
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
