use std::net::IpAddr;

use nonproxy_model::ConnectionContext;

use crate::{CompiledRule, index::preferred_matching_rule};

#[derive(Clone, Debug, Default)]
pub(crate) struct CidrRuleIndex {
    ipv4: RadixNode,
    ipv6: RadixNode,
}

#[derive(Clone, Debug, Default)]
struct RadixNode {
    rules: Vec<CompiledRule>,
    zero: Option<Box<Self>>,
    one: Option<Box<Self>>,
}

impl CidrRuleIndex {
    pub(crate) fn insert(&mut self, rule: CompiledRule) {
        let Some(cidr) = rule.matcher().cidr() else {
            return;
        };
        match cidr.network() {
            IpAddr::V4(address) => {
                self.ipv4.insert(
                    u128::from(u32::from(address)),
                    cidr.prefix_length(),
                    32,
                    rule,
                );
            }
            IpAddr::V6(address) => {
                self.ipv6
                    .insert(u128::from(address), cidr.prefix_length(), 128, rule);
            }
        }
    }

    pub(crate) fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        let address = context.destination().ip()?;
        let mut candidates = Vec::new();
        match address {
            IpAddr::V4(value) => {
                self.ipv4
                    .collect(u128::from(u32::from(value)), 32, &mut candidates)
            }
            IpAddr::V6(value) => {
                self.ipv6.collect(u128::from(value), 128, &mut candidates);
            }
        }
        preferred_matching_rule(candidates, context)
    }
}

impl RadixNode {
    fn insert(&mut self, address: u128, prefix_length: u8, address_bits: u8, rule: CompiledRule) {
        let mut node = self;
        for offset in 0..prefix_length {
            let bit = bit_at(address, address_bits, offset);
            let branch = if bit == 0 {
                &mut node.zero
            } else {
                &mut node.one
            };
            node = branch
                .get_or_insert_with(|| Box::new(Self::default()))
                .as_mut();
        }
        node.rules.push(rule);
    }

    fn collect<'a>(
        &'a self,
        address: u128,
        address_bits: u8,
        candidates: &mut Vec<&'a CompiledRule>,
    ) {
        candidates.extend(&self.rules);
        let mut node = self;
        for offset in 0..address_bits {
            let branch = if bit_at(address, address_bits, offset) == 0 {
                &node.zero
            } else {
                &node.one
            };
            let Some(child) = branch.as_deref() else {
                break;
            };
            node = child;
            candidates.extend(&node.rules);
        }
    }
}

fn bit_at(address: u128, address_bits: u8, offset: u8) -> u128 {
    (address >> (address_bits - offset - 1)) & 1
}
