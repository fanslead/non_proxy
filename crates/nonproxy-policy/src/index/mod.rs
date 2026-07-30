mod app;
mod app_destination;
mod cidr;
mod domain;
mod network;

pub(crate) use app::AppRuleIndex;
pub(crate) use app_destination::AppDestinationRuleIndex;
pub(crate) use cidr::CidrRuleIndex;
pub(crate) use domain::DomainRuleIndex;
pub(crate) use network::NetworkRuleIndex;

use nonproxy_model::ConnectionContext;

use crate::CompiledRule;

pub(crate) fn preferred_matching_rule<'a>(
    candidates: impl IntoIterator<Item = &'a CompiledRule>,
    context: &ConnectionContext,
) -> Option<&'a CompiledRule> {
    candidates
        .into_iter()
        .filter(|rule| rule.matches(context))
        .reduce(|current, candidate| {
            if candidate.is_preferred_to(current) {
                candidate
            } else {
                current
            }
        })
}

pub(crate) fn prefer_optional_rules<'a>(
    first: Option<&'a CompiledRule>,
    second: Option<&'a CompiledRule>,
) -> Option<&'a CompiledRule> {
    match (first, second) {
        (Some(first), Some(second)) => {
            if second.is_preferred_to(first) {
                Some(second)
            } else {
                Some(first)
            }
        }
        (Some(rule), None) | (None, Some(rule)) => Some(rule),
        (None, None) => None,
    }
}
