use std::collections::HashSet;

use url::{Host, Url};

use crate::outbound_import::{OutboundImportError, RawOutbound, RawOutboundKind};

const MAXIMUM_URI_COUNT: usize = 100;
const MAXIMUM_IDENTIFIER_LENGTH: usize = 128;

pub(crate) fn parse(configuration: &[u8]) -> Result<Vec<RawOutbound>, OutboundImportError> {
    let source = std::str::from_utf8(configuration).map_err(|_| OutboundImportError::Invalid)?;
    let lines = source
        .lines()
        .enumerate()
        .filter_map(|(index, value)| {
            let value = value.trim();
            (!value.is_empty()).then_some((index + 1, value))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > MAXIMUM_URI_COUNT {
        return Err(OutboundImportError::Invalid);
    }

    let mut identifiers = HashSet::new();
    lines
        .into_iter()
        .enumerate()
        .map(|(index, (line, value))| parse_line(value, line, index + 1, &mut identifiers))
        .collect()
}

fn parse_line(
    value: &str,
    line: usize,
    ordinal: usize,
    identifiers: &mut HashSet<String>,
) -> Result<RawOutbound, OutboundImportError> {
    let uri = Url::parse(value).map_err(|_| OutboundImportError::UriInvalid { line })?;
    let (kind, default_port) = match uri.scheme() {
        "socks5" | "socks5h" => (RawOutboundKind::Socks5, 1_080),
        "http" => (RawOutboundKind::HttpConnect, 80),
        _ => return Err(OutboundImportError::UriSchemeUnsupported { line }),
    };
    if !matches!(uri.path(), "" | "/") || uri.query().is_some() {
        return Err(OutboundImportError::UriInvalid { line });
    }
    let host = match uri.host() {
        Some(Host::Domain(value)) if !value.is_empty() => value.to_owned(),
        Some(Host::Ipv4(value)) => value.to_string(),
        Some(Host::Ipv6(value)) => value.to_string(),
        _ => return Err(OutboundImportError::UriInvalid { line }),
    };
    let port = uri.port().unwrap_or(default_port);
    if port == 0 {
        return Err(OutboundImportError::UriInvalid { line });
    }
    let (username, password) = credentials(&uri, line)?;
    let label = uri
        .fragment()
        .filter(|value| !value.is_empty())
        .map(|value| decode_component(value).ok_or(OutboundImportError::UriInvalid { line }))
        .transpose()?;
    let fallback = format!(
        "{}-{host}-{port}",
        match kind {
            RawOutboundKind::Socks5 => "socks5",
            RawOutboundKind::HttpConnect => "http",
        }
    );
    let id = unique_identifier(label.as_deref().unwrap_or(&fallback), ordinal, identifiers);

    Ok(RawOutbound {
        id,
        kind,
        host,
        port,
        username,
        password,
        enabled: true,
    })
}

fn credentials(
    uri: &Url,
    line: usize,
) -> Result<(Option<String>, Option<String>), OutboundImportError> {
    if uri.username().is_empty() && uri.password().is_none() {
        return Ok((None, None));
    }
    let username = decode_component(uri.username())
        .filter(|value| valid_credential_field(value))
        .ok_or(OutboundImportError::UriCredentialInvalid { line })?;
    let password = uri
        .password()
        .and_then(decode_component)
        .filter(|value| valid_credential_field(value))
        .ok_or(OutboundImportError::UriCredentialInvalid { line })?;
    Ok((Some(username), Some(password)))
}

fn decode_component(value: &str) -> Option<String> {
    if !valid_percent_encoding(value.as_bytes()) {
        return None;
    }
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|value| value.into_owned())
}

fn valid_credential_field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}

fn valid_percent_encoding(value: &[u8]) -> bool {
    let mut index = 0;
    while index < value.len() {
        if value[index] == b'%' {
            if index + 2 >= value.len()
                || !value[index + 1].is_ascii_hexdigit()
                || !value[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn unique_identifier(value: &str, ordinal: usize, identifiers: &mut HashSet<String>) -> String {
    let mut base = slug(value);
    if base.is_empty() {
        base = format!("imported-proxy-{ordinal}");
    }
    if identifiers.insert(base.clone()) {
        return base;
    }
    for suffix in 2..=MAXIMUM_URI_COUNT {
        let suffix = format!("-{suffix}");
        let available = MAXIMUM_IDENTIFIER_LENGTH.saturating_sub(suffix.len());
        let candidate = format!("{}{}", &base[..base.len().min(available)], suffix);
        if identifiers.insert(candidate.clone()) {
            return candidate;
        }
    }
    format!("imported-proxy-{ordinal}")
}

fn slug(value: &str) -> String {
    let mut result = String::with_capacity(value.len().min(MAXIMUM_IDENTIFIER_LENGTH));
    let mut separator = false;
    for byte in value.bytes() {
        if result.len() >= MAXIMUM_IDENTIFIER_LENGTH {
            break;
        }
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_') {
            result.push(char::from(byte.to_ascii_lowercase()));
            separator = false;
        } else if !result.is_empty() && !separator {
            result.push('-');
            separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_common_links_decodes_credentials_and_generates_unique_safe_ids() {
        let values = parse(
            b"socks5://alice:p%40ss@proxy.example:1080#Office%20Proxy\n\
               http://127.0.0.1:8080#Office%20Proxy",
        )
        .unwrap_or_else(|error| panic!("标准链接解析失败: {error}"));

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].id, "office-proxy");
        assert_eq!(values[1].id, "office-proxy-2");
        assert_eq!(values[0].username.as_deref(), Some("alice"));
        assert_eq!(values[0].password.as_deref(), Some("p@ss"));
        assert_eq!(values[0].host, "proxy.example");
        assert_eq!(values[1].port, 8_080);
    }

    #[test]
    fn applies_conventional_default_ports_and_normalizes_ipv6_hosts() {
        let values = parse(b"socks5://[2001:db8::1]#IPv6\nhttp://proxy.example#")
            .unwrap_or_else(|error| panic!("默认端口链接解析失败: {error}"));

        assert_eq!(values[0].host, "2001:db8::1");
        assert_eq!(values[0].port, 1_080);
        assert_eq!(values[1].host, "proxy.example");
        assert_eq!(values[1].port, 80);
        assert_eq!(values[1].id, "http-proxy.example-80");
    }

    #[test]
    fn rejects_dangerous_shapes_without_echoing_input() {
        let cases = [
            "https://secret.example:443",
            "socks5://secret.example:1080/path",
            "socks5://secret.example:1080?token=private",
            "socks5://alice@secret.example:1080",
            "socks5://alice:p%ZZ@secret.example:1080",
        ];

        for value in cases {
            let error = parse(value.as_bytes())
                .err()
                .unwrap_or_else(|| panic!("危险链接应被拒绝: {value}"));
            assert!(!error.to_string().contains("secret"));
            assert!(!error.to_string().contains("private"));
        }
    }
}
