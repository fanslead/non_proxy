use std::fmt;

use url::{Host, Url};
use zeroize::Zeroizing;

use crate::SubscriptionFetchError;

const DEFAULT_HTTPS_PORT: u16 = 443;
const MAXIMUM_ENDPOINT_BYTES: usize = 4 * 1024;

pub struct SubscriptionEndpoint {
    host: Zeroizing<String>,
    port: u16,
    path_and_query: Zeroizing<String>,
}

impl SubscriptionEndpoint {
    pub fn parse(value: &str) -> Result<Self, SubscriptionFetchError> {
        if value.is_empty()
            || value.len() > MAXIMUM_ENDPOINT_BYTES
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_ascii_control())
        {
            return Err(SubscriptionFetchError::EndpointInvalid);
        }
        let url = Url::parse(value).map_err(|_| SubscriptionFetchError::EndpointInvalid)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || original_authority(value).is_none_or(|value| value.contains('@'))
        {
            return Err(SubscriptionFetchError::EndpointInvalid);
        }
        let host = match url.host() {
            Some(Host::Domain(value)) => value.strip_suffix('.').unwrap_or(value).to_owned(),
            Some(Host::Ipv4(value)) => value.to_string(),
            Some(Host::Ipv6(value)) => value.to_string(),
            None => return Err(SubscriptionFetchError::EndpointInvalid),
        };
        if host.is_empty() {
            return Err(SubscriptionFetchError::EndpointInvalid);
        }
        let port = url
            .port_or_known_default()
            .filter(|value| *value > 0)
            .ok_or(SubscriptionFetchError::EndpointInvalid)?;
        let mut path_and_query = Zeroizing::new(match url.path() {
            "" => "/".to_owned(),
            value => value.to_owned(),
        });
        if let Some(query) = url.query() {
            path_and_query.push('?');
            path_and_query.push_str(query);
        }
        Ok(Self {
            host: Zeroizing::new(host),
            port,
            path_and_query,
        })
    }

    #[must_use]
    pub(crate) fn host(&self) -> &str {
        self.host.as_str()
    }

    #[must_use]
    pub(crate) const fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn path_and_query(&self) -> &str {
        &self.path_and_query
    }

    pub(crate) fn host_header(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host.as_str())
        } else {
            self.host.to_string()
        };
        if self.port == DEFAULT_HTTPS_PORT {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

fn original_authority(value: &str) -> Option<&str> {
    value.split_once("://")?.1.split(['/', '?', '#']).next()
}

impl fmt::Debug for SubscriptionEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionEndpoint")
            .field("url", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriptionEndpoint;

    #[test]
    fn accepts_https_tokens_without_exposing_them_in_debug_output() {
        let endpoint =
            SubscriptionEndpoint::parse("https://Subscription.Example:8443/nodes?token=private")
                .unwrap_or_else(|error| panic!("合法订阅地址解析失败: {error}"));

        assert_eq!(endpoint.host(), "subscription.example");
        assert_eq!(endpoint.port(), 8_443);
        assert_eq!(endpoint.host_header(), "subscription.example:8443");
        assert_eq!(endpoint.path_and_query(), "/nodes?token=private");
        let metadata = format!("{endpoint:?}");
        assert!(!metadata.contains("nodes"));
        assert!(!metadata.contains("private"));
        assert!(!metadata.contains("subscription.example"));
    }

    #[test]
    fn rejects_non_https_ambiguous_or_credential_bearing_addresses() {
        for value in [
            "http://subscription.example/nodes",
            "https://user@subscription.example/nodes",
            "https://@subscription.example/nodes",
            "https://subscription.example/nodes#fragment",
            " https://subscription.example/nodes",
            "https://subscription.example/a b",
            "https://subscription.example/nodes\n",
            "https://subscription.example/nodes\u{7f}",
        ] {
            assert!(SubscriptionEndpoint::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn canonicalizes_ipv6_without_creating_a_double_bracket_host_header() {
        let endpoint = SubscriptionEndpoint::parse("https://[2606:4700:4700::1111]:8443/nodes")
            .unwrap_or_else(|error| panic!("IPv6 订阅地址解析失败: {error}"));

        assert_eq!(endpoint.host(), "2606:4700:4700::1111");
        assert_eq!(endpoint.host_header(), "[2606:4700:4700::1111]:8443");
    }
}
