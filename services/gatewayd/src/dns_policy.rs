use nonproxy_dns::{ParsedDnsQuery, SyntheticAddressFamily};
use nonproxy_model::{AppIdentity, DecisionSpec, Destination, DomainName, Platform, Transport};
use nonproxy_policy::{CompiledPolicySnapshot, PolicyEngine};

const DNS_PORT: u16 = 53;
const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;
const QTYPE_SVCB: u16 = 64;
const QTYPE_HTTPS: u16 = 65;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DnsQueryPlan {
    Synthetic {
        domain: DomainName,
        family: SyntheticAddressFamily,
    },
    NoData,
    Route(DecisionSpec),
}

pub(crate) fn plan_query(
    snapshot: &CompiledPolicySnapshot,
    query: &ParsedDnsQuery,
) -> DnsQueryPlan {
    let Ok(domain) = DomainName::normalize(query.question().qname().as_ascii()) else {
        return DnsQueryPlan::Route(snapshot.default_decision().clone());
    };
    if snapshot.requires_domain_identity(&domain) {
        match query.question().qtype() {
            QTYPE_A => {
                return DnsQueryPlan::Synthetic {
                    domain,
                    family: SyntheticAddressFamily::Ipv4,
                };
            }
            QTYPE_AAAA => {
                return DnsQueryPlan::Synthetic {
                    domain,
                    family: SyntheticAddressFamily::Ipv6,
                };
            }
            QTYPE_SVCB | QTYPE_HTTPS => return DnsQueryPlan::NoData,
            _ => {}
        }
    }
    let destination = Destination::new(Some(domain.as_ascii()), None, DNS_PORT, Transport::Udp);
    let Ok(destination) = destination else {
        return DnsQueryPlan::Route(snapshot.default_decision().clone());
    };
    let context = nonproxy_model::ConnectionContext::new(
        AppIdentity::unknown(Platform::Windows),
        destination,
    );
    DnsQueryPlan::Route(PolicyEngine::decide(snapshot, &context).result().clone())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use hickory_proto::{
        op::{Message, MessageType, OpCode, Query},
        rr::{Name, RecordType},
    };
    use nonproxy_model::{
        DecisionSpec, DomainMatchKind, DomainMatcher, FailureMode, OutboundId, Policy, PolicyId,
        PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind, RouteAction,
    };
    use nonproxy_policy::OutboundCapabilities;
    use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};

    use super::*;

    fn query(name: &str, record_type: RecordType) -> Result<ParsedDnsQuery, Box<dyn Error>> {
        let mut message = Message::new(7, MessageType::Query, OpCode::Query);
        message.add_query(Query::query(Name::from_ascii(name)?, record_type));
        Ok(ParsedDnsQuery::parse(&message.to_vec()?)?)
    }

    fn snapshot() -> Result<CompiledPolicySnapshot, Box<dyn Error>> {
        let matcher = PolicyMatch::new(
            None,
            Some(DomainMatcher::new(
                DomainMatchKind::Suffix,
                "direct.example",
            )?),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )?;
        let policy = Policy::new(
            PolicyId::new("direct-site")?,
            "直连站点",
            matcher,
            DecisionSpec::direct(),
            PolicyMetadata::new(PolicySourceKind::Site, 10, PolicyOrigin::User, 1),
        )?;
        let outbound = OutboundId::new("proxy")?;
        Ok(PolicyCompiler::compile(CompileRequest::new(
            1,
            1_000,
            DecisionSpec::new(
                RouteAction::Proxy,
                Some(outbound.clone()),
                FailureMode::Closed,
            )?,
            vec![policy],
            CompileCapabilities::full().with_outbound(outbound, OutboundCapabilities::full()),
        ))?)
    }

    #[test]
    fn address_queries_for_domain_rules_receive_family_specific_synthetic_plan()
    -> Result<(), Box<dyn Error>> {
        let snapshot = snapshot()?;

        assert!(matches!(
            plan_query(&snapshot, &query("api.direct.example.", RecordType::A)?),
            DnsQueryPlan::Synthetic {
                family: SyntheticAddressFamily::Ipv4,
                ..
            }
        ));
        assert!(matches!(
            plan_query(&snapshot, &query("api.direct.example.", RecordType::AAAA)?),
            DnsQueryPlan::Synthetic {
                family: SyntheticAddressFamily::Ipv6,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn https_query_is_nodata_and_unmatched_domain_uses_default_route() -> Result<(), Box<dyn Error>>
    {
        let snapshot = snapshot()?;

        assert_eq!(
            plan_query(&snapshot, &query("api.direct.example.", RecordType::HTTPS)?),
            DnsQueryPlan::NoData
        );
        let DnsQueryPlan::Route(decision) =
            plan_query(&snapshot, &query("other.example.", RecordType::A)?)
        else {
            return Err("未命中域名应使用默认路由".into());
        };
        assert_eq!(decision.action(), RouteAction::Proxy);
        Ok(())
    }
}
