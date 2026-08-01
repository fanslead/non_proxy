use nonproxy_dns::{ParsedDnsQuery, SyntheticAddressFamily};
use nonproxy_model::{
    AppIdentity, ConnectionContext, Decision, Destination, DomainName, Platform,
    RuntimeOverrideMode, Transport,
};
use nonproxy_policy::{CompiledPolicySnapshot, PolicyEngine, PolicyEvaluation};

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
    System {
        snapshot_version: u64,
    },
    Route(Box<DnsRouteDecision>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DnsRouteDecision {
    pub context: Option<ConnectionContext>,
    pub decision: Decision,
}

pub(crate) fn plan_query_at(
    snapshot: &CompiledPolicySnapshot,
    query: &ParsedDnsQuery,
    unix_time_ms: u64,
) -> DnsQueryPlan {
    let Ok(domain) = DomainName::normalize(query.question().qname().as_ascii()) else {
        return default_route_at(snapshot, unix_time_ms);
    };
    let destination = Destination::new(Some(domain.as_ascii()), None, DNS_PORT, Transport::Udp);
    let Ok(destination) = destination else {
        return default_route_at(snapshot, unix_time_ms);
    };
    let context = ConnectionContext::new(AppIdentity::unknown(Platform::Windows), destination);
    let evaluation = PolicyEngine::evaluate_at(snapshot, &context, unix_time_ms);
    match &evaluation {
        PolicyEvaluation::Bypass {
            snapshot_version, ..
        } => {
            return DnsQueryPlan::System {
                snapshot_version: *snapshot_version,
            };
        }
        PolicyEvaluation::Decision(decision)
            if decision.reason_code() == "NP_POLICY_SYSTEM_MATCH"
                || decision.reason_code().starts_with("NP_RUNTIME_OVERRIDE_") =>
        {
            return route(context, decision.clone());
        }
        PolicyEvaluation::Decision(_) => {}
    }
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
    let PolicyEvaluation::Decision(decision) = evaluation else {
        unreachable!("旁路判定已提前返回")
    };
    route(context, decision)
}

fn route(context: ConnectionContext, decision: Decision) -> DnsQueryPlan {
    DnsQueryPlan::Route(Box::new(DnsRouteDecision {
        context: Some(context),
        decision,
    }))
}

fn default_route_at(snapshot: &CompiledPolicySnapshot, unix_time_ms: u64) -> DnsQueryPlan {
    if snapshot.runtime_override().is_some_and(|value| {
        value.is_active_at(unix_time_ms) && value.mode() == RuntimeOverrideMode::Paused
    }) {
        return DnsQueryPlan::System {
            snapshot_version: snapshot.metadata().snapshot_version(),
        };
    }
    DnsQueryPlan::Route(Box::new(DnsRouteDecision {
        context: None,
        decision: Decision::defaulted(
            snapshot.default_decision().clone(),
            snapshot.metadata().snapshot_version(),
            "NP_POLICY_DEFAULT",
        ),
    }))
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
        RuntimeOverrideMode, RuntimeRoutingOverride,
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

    fn snapshot_with_override(
        runtime_override: RuntimeRoutingOverride,
    ) -> Result<CompiledPolicySnapshot, Box<dyn Error>> {
        let outbound = OutboundId::new("proxy")?;
        Ok(PolicyCompiler::compile(
            CompileRequest::new(
                1,
                1_000,
                DecisionSpec::new(
                    RouteAction::Proxy,
                    Some(outbound.clone()),
                    FailureMode::Closed,
                )?,
                Vec::new(),
                CompileCapabilities::full().with_outbound(outbound, OutboundCapabilities::full()),
            )
            .with_runtime_override(Some(runtime_override)),
        )?)
    }

    #[test]
    fn address_queries_for_domain_rules_receive_family_specific_synthetic_plan()
    -> Result<(), Box<dyn Error>> {
        let snapshot = snapshot()?;

        assert!(matches!(
            plan_query_at(
                &snapshot,
                &query("api.direct.example.", RecordType::A)?,
                1_000
            ),
            DnsQueryPlan::Synthetic {
                family: SyntheticAddressFamily::Ipv4,
                ..
            }
        ));
        assert!(matches!(
            plan_query_at(
                &snapshot,
                &query("api.direct.example.", RecordType::AAAA)?,
                1_000
            ),
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
            plan_query_at(
                &snapshot,
                &query("api.direct.example.", RecordType::HTTPS)?,
                1_000
            ),
            DnsQueryPlan::NoData
        );
        let DnsQueryPlan::Route(route) =
            plan_query_at(&snapshot, &query("other.example.", RecordType::A)?, 1_000)
        else {
            return Err("未命中域名应使用默认路由".into());
        };
        assert_eq!(route.decision.result().action(), RouteAction::Proxy);
        Ok(())
    }

    #[test]
    fn paused_override_uses_system_dns_until_expiry() -> Result<(), Box<dyn Error>> {
        let snapshot = snapshot_with_override(RuntimeRoutingOverride::new(
            RuntimeOverrideMode::Paused,
            None,
            2_000,
        )?)?;

        assert_eq!(
            plan_query_at(&snapshot, &query("example.com.", RecordType::A)?, 1_999),
            DnsQueryPlan::System {
                snapshot_version: 1
            }
        );
        let DnsQueryPlan::Route(expired) =
            plan_query_at(&snapshot, &query("example.com.", RecordType::A)?, 2_000)
        else {
            return Err("暂停到期后应恢复默认 DNS 路由".into());
        };
        assert_eq!(expired.decision.result().action(), RouteAction::Proxy);
        Ok(())
    }

    #[test]
    fn direct_override_routes_dns_instead_of_returning_synthetic_answers()
    -> Result<(), Box<dyn Error>> {
        let snapshot = snapshot_with_override(RuntimeRoutingOverride::new(
            RuntimeOverrideMode::Direct,
            None,
            2_000,
        )?)?;

        let DnsQueryPlan::Route(route) =
            plan_query_at(&snapshot, &query("example.com.", RecordType::A)?, 1_999)
        else {
            return Err("全部直连覆盖应路由真实 DNS 查询".into());
        };
        assert_eq!(route.decision.result().action(), RouteAction::Direct);
        assert_eq!(route.decision.reason_code(), "NP_RUNTIME_OVERRIDE_DIRECT");
        Ok(())
    }
}
