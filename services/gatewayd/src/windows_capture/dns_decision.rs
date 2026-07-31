use nonproxy_model::{ConnectionContext, Decision, OutboundId};

use crate::decision_event::{
    DecisionEventReporter, DecisionObservation, ObservedPath, new_flow_id, observed_dns_path,
};

pub struct WindowsDnsObservation {
    reporter: DecisionEventReporter,
    observation: Option<DecisionObservation>,
}

impl WindowsDnsObservation {
    pub fn new(
        reporter: DecisionEventReporter,
        provider_generation: u64,
        context: Option<ConnectionContext>,
        decision: Decision,
        observed_at_unix_ms: u64,
        decision_latency_micros: u64,
    ) -> Self {
        let observation = context.map(|context| {
            DecisionObservation::new(
                "windows-dns",
                provider_generation,
                new_flow_id("dns"),
                observed_at_unix_ms,
                context,
                decision,
                decision_latency_micros,
            )
        });
        Self {
            reporter,
            observation,
        }
    }

    pub fn decision(&self) {
        self.report(ObservedPath::Decision, None)
    }

    pub fn failed(&self, error_code: &'static str) {
        self.report(ObservedPath::Decision, Some(error_code))
    }

    pub fn direct(&self, interface_index: u32, cache_hit: bool, fail_open: bool) {
        let path = observed_dns_path(
            cache_hit,
            ObservedPath::Direct {
                interface_index,
                fail_open,
            },
        );
        let error_code = if fail_open && cache_hit {
            Some("NP_WINDOWS_DNS_PROXY_FAIL_OPEN_CACHE_HIT")
        } else {
            (fail_open && !cache_hit).then_some("NP_WINDOWS_DNS_PROXY_FAIL_OPEN_DIRECT")
        };
        self.report(path, error_code)
    }

    pub fn proxy(&self, outbound_id: OutboundId, cache_hit: bool) {
        self.report(
            observed_dns_path(cache_hit, ObservedPath::Proxy { outbound_id }),
            None,
        )
    }

    fn report(&self, path: ObservedPath, error_code: Option<&str>) {
        let Some(observation) = self.observation.as_ref() else {
            return;
        };
        match observation.record(path, error_code) {
            Ok(decision) => {
                let _accepted = self.reporter.submit(decision);
            }
            Err(_) => self.reporter.record_unreportable(),
        }
    }
}
