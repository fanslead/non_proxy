use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use zeroize::Zeroizing;

use crate::outbound_import::{MAX_IMPORT_BYTES, OutboundImportError, RawOutbound, RawOutboundKind};

pub(crate) fn parse(configuration: &[u8]) -> Result<Vec<RawOutbound>, OutboundImportError> {
    let encoded = Zeroizing::new(
        configuration
            .iter()
            .copied()
            .filter(|value| !value.is_ascii_whitespace())
            .collect::<Vec<_>>(),
    );
    if encoded.is_empty() {
        return Err(OutboundImportError::SubscriptionEncoding);
    }

    let decoded = decode(encoded.as_slice())?;
    if decoded.is_empty() || decoded.len() > MAX_IMPORT_BYTES {
        return Err(OutboundImportError::SubscriptionEncoding);
    }
    let outbounds = crate::outbound_import_uri::parse(decoded.as_slice())?;
    if outbounds
        .iter()
        .any(|value| !matches!(value.kind, RawOutboundKind::Shadowsocks))
    {
        return Err(OutboundImportError::SubscriptionContent);
    }
    Ok(outbounds)
}

fn decode(encoded: &[u8]) -> Result<Zeroizing<Vec<u8>>, OutboundImportError> {
    [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD_NO_PAD, &STANDARD]
        .into_iter()
        .find_map(|engine| engine.decode(encoded).ok())
        .map(Zeroizing::new)
        .ok_or(OutboundImportError::SubscriptionEncoding)
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose};

    use super::parse;
    use crate::outbound_import::OutboundImportError;

    #[test]
    fn parses_standard_and_url_safe_subscription_payloads() {
        let links = b"ss://YWVzLTI1Ni1nY206cHJpdmF0ZQ@one.example:8388#One\n\
                      ss://YWVzLTEyOC1nY206c2VjcmV0@two.example:8389#Two";
        for payload in [
            general_purpose::STANDARD.encode(links),
            general_purpose::URL_SAFE_NO_PAD.encode(links),
        ] {
            let values = parse(payload.as_bytes())
                .unwrap_or_else(|error| panic!("订阅内容解析失败: {error}"));

            assert_eq!(values.len(), 2);
            assert_eq!(values[0].id, "one");
            assert_eq!(values[1].host, "two.example");
        }
    }

    #[test]
    fn accepts_wrapped_base64_but_rejects_invalid_or_mixed_protocol_content() {
        let link = b"ss://YWVzLTI1Ni1nY206cHJpdmF0ZQ@one.example:8388#One";
        let encoded = general_purpose::STANDARD.encode(link);
        let wrapped = format!(" {}\n{} ", &encoded[..12], &encoded[12..]);
        assert!(parse(wrapped.as_bytes()).is_ok());

        assert!(matches!(
            parse(b"not-base64!"),
            Err(OutboundImportError::SubscriptionEncoding)
        ));
        let mixed = general_purpose::STANDARD.encode(
            b"ss://YWVzLTI1Ni1nY206cHJpdmF0ZQ@one.example:8388#One\n\
              socks5://proxy.example:1080#Other",
        );
        assert!(matches!(
            parse(mixed.as_bytes()),
            Err(OutboundImportError::SubscriptionContent)
        ));
    }
}
