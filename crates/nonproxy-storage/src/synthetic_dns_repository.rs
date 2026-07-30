use std::{
    collections::HashSet,
    net::{IpAddr, Ipv6Addr},
};

use nonproxy_dns::{SYNTHETIC_IPV4_CAPACITY, SyntheticAddressFamily, SyntheticAddressSpace};
use nonproxy_model::DomainName;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::StorageError;

pub const SYNTHETIC_BINDING_RETENTION_MS: u64 = 24 * 60 * 60 * 1_000;
const MAXIMUM_LISTED_BINDINGS: usize = (SYNTHETIC_IPV4_CAPACITY as usize) * 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntheticDnsBinding {
    domain: DomainName,
    address: IpAddr,
    retain_until_unix_ms: u64,
}

impl SyntheticDnsBinding {
    #[must_use]
    pub const fn domain(&self) -> &DomainName {
        &self.domain
    }

    #[must_use]
    pub const fn address(&self) -> IpAddr {
        self.address
    }

    #[must_use]
    pub const fn retain_until_unix_ms(&self) -> u64 {
        self.retain_until_unix_ms
    }
}

pub struct SyntheticDnsRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> SyntheticDnsRepository<'connection> {
    pub(crate) const fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn load_or_create_space(
        &mut self,
        proposed_ipv6_prefix: Ipv6Addr,
        now_unix_ms: u64,
    ) -> Result<SyntheticAddressSpace, StorageError> {
        let proposed = SyntheticAddressSpace::new(proposed_ipv6_prefix)
            .map_err(|_| StorageError::SyntheticDnsConfigInvalid)?;
        let stored = self
            .connection
            .query_row(
                "SELECT ipv6_prefix FROM synthetic_dns_config WHERE singleton_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(stored) = stored {
            return parse_space(&stored);
        }
        self.connection.execute(
            "INSERT INTO synthetic_dns_config(
                 singleton_id, ipv6_prefix, created_at_unix_ms
             ) VALUES (1, ?1, ?2)",
            params![proposed_ipv6_prefix.to_string(), sqlite_u64(now_unix_ms)?],
        )?;
        Ok(proposed)
    }

    pub fn get_or_create(
        &mut self,
        space: SyntheticAddressSpace,
        domain: &DomainName,
        family: SyntheticAddressFamily,
        now_unix_ms: u64,
    ) -> Result<SyntheticDnsBinding, StorageError> {
        let retain_until = now_unix_ms
            .checked_add(SYNTHETIC_BINDING_RETENTION_MS)
            .ok_or(StorageError::SyntheticDnsConfigInvalid)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM synthetic_dns_binding WHERE retain_until_unix_ms <= ?1",
            [sqlite_u64(now_unix_ms)?],
        )?;
        if let Some(existing) = binding_for_domain(&transaction, domain, family)? {
            if family_for_address(existing) != family || !space.contains(existing) {
                return Err(StorageError::CorruptData {
                    field: "synthetic_dns_binding.address",
                });
            }
            transaction.execute(
                "UPDATE synthetic_dns_binding
                 SET last_issued_at_unix_ms = ?1, retain_until_unix_ms = ?2
                 WHERE family = ?3 AND domain_ascii = ?4",
                params![
                    sqlite_u64(now_unix_ms)?,
                    sqlite_u64(retain_until)?,
                    family_value(family),
                    domain.as_ascii()
                ],
            )?;
            transaction.commit()?;
            return Ok(SyntheticDnsBinding {
                domain: domain.clone(),
                address: existing,
                retain_until_unix_ms: retain_until,
            });
        }
        let occupied = occupied_addresses(&transaction, family)?;
        let address = first_available(space, domain, family, &occupied)?;
        transaction.execute(
            "INSERT INTO synthetic_dns_binding(
                 family, address, domain_ascii, created_at_unix_ms,
                 last_issued_at_unix_ms, retain_until_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
            params![
                family_value(family),
                address.to_string(),
                domain.as_ascii(),
                sqlite_u64(now_unix_ms)?,
                sqlite_u64(retain_until)?
            ],
        )?;
        transaction.commit()?;
        Ok(SyntheticDnsBinding {
            domain: domain.clone(),
            address,
            retain_until_unix_ms: retain_until,
        })
    }

    pub fn lookup(
        &self,
        space: SyntheticAddressSpace,
        address: IpAddr,
        now_unix_ms: u64,
    ) -> Result<Option<SyntheticDnsBinding>, StorageError> {
        let family = family_for_address(address);
        let row = self
            .connection
            .query_row(
                "SELECT domain_ascii, retain_until_unix_ms
                 FROM synthetic_dns_binding
                 WHERE family = ?1 AND address = ?2 AND retain_until_unix_ms > ?3",
                params![
                    family_value(family),
                    address.to_string(),
                    sqlite_u64(now_unix_ms)?
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        row.map(|(domain, retain_until)| binding_from_row(space, domain, address, retain_until))
            .transpose()
    }

    pub fn list_retained(
        &self,
        space: SyntheticAddressSpace,
        now_unix_ms: u64,
        limit: usize,
    ) -> Result<Vec<SyntheticDnsBinding>, StorageError> {
        if limit == 0 || limit > MAXIMUM_LISTED_BINDINGS {
            return Err(StorageError::SyntheticDnsLimitInvalid);
        }
        let mut statement = self.connection.prepare(
            "SELECT domain_ascii, address, retain_until_unix_ms
             FROM synthetic_dns_binding
             WHERE retain_until_unix_ms > ?1
             ORDER BY family, address
             LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![sqlite_u64(now_unix_ms)?, sqlite_usize(limit)?],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (domain, address, retain_until) = row?;
            let address = address.parse().map_err(|_| StorageError::CorruptData {
                field: "synthetic_dns_binding.address",
            })?;
            binding_from_row(space, domain, address, retain_until)
        })
        .collect()
    }
}

fn binding_for_domain(
    transaction: &Transaction<'_>,
    domain: &DomainName,
    family: SyntheticAddressFamily,
) -> Result<Option<IpAddr>, StorageError> {
    let value = transaction
        .query_row(
            "SELECT address FROM synthetic_dns_binding
             WHERE family = ?1 AND domain_ascii = ?2",
            params![family_value(family), domain.as_ascii()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value
        .map(|value| {
            value.parse().map_err(|_| StorageError::CorruptData {
                field: "synthetic_dns_binding.address",
            })
        })
        .transpose()
}

fn occupied_addresses(
    transaction: &Transaction<'_>,
    family: SyntheticAddressFamily,
) -> Result<HashSet<IpAddr>, StorageError> {
    let mut statement =
        transaction.prepare("SELECT address FROM synthetic_dns_binding WHERE family = ?1")?;
    let rows = statement.query_map([family_value(family)], |row| row.get::<_, String>(0))?;
    rows.map(|row| {
        row?.parse().map_err(|_| StorageError::CorruptData {
            field: "synthetic_dns_binding.address",
        })
    })
    .collect()
}

fn first_available(
    space: SyntheticAddressSpace,
    domain: &DomainName,
    family: SyntheticAddressFamily,
    occupied: &HashSet<IpAddr>,
) -> Result<IpAddr, StorageError> {
    for attempt in 0..space.capacity(family) {
        let candidate = space
            .candidate(domain, family, attempt)
            .map_err(|_| StorageError::SyntheticDnsAddressExhausted)?;
        if !occupied.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(StorageError::SyntheticDnsAddressExhausted)
}

fn binding_from_row(
    space: SyntheticAddressSpace,
    domain: String,
    address: IpAddr,
    retain_until: i64,
) -> Result<SyntheticDnsBinding, StorageError> {
    if !space.contains(address) {
        return Err(StorageError::CorruptData {
            field: "synthetic_dns_binding.address",
        });
    }
    Ok(SyntheticDnsBinding {
        domain: DomainName::normalize(&domain)?,
        address,
        retain_until_unix_ms: from_sqlite_u64(
            retain_until,
            "synthetic_dns_binding.retain_until_unix_ms",
        )?,
    })
}

fn parse_space(value: &str) -> Result<SyntheticAddressSpace, StorageError> {
    let prefix = value
        .parse()
        .map_err(|_| StorageError::SyntheticDnsConfigInvalid)?;
    SyntheticAddressSpace::new(prefix).map_err(|_| StorageError::SyntheticDnsConfigInvalid)
}

const fn family_value(family: SyntheticAddressFamily) -> i64 {
    match family {
        SyntheticAddressFamily::Ipv4 => 4,
        SyntheticAddressFamily::Ipv6 => 6,
    }
}

const fn family_for_address(address: IpAddr) -> SyntheticAddressFamily {
    match address {
        IpAddr::V4(_) => SyntheticAddressFamily::Ipv4,
        IpAddr::V6(_) => SyntheticAddressFamily::Ipv6,
    }
}

fn sqlite_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::CorruptData {
        field: "synthetic_dns_timestamp",
    })
}

fn sqlite_usize(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::SyntheticDnsLimitInvalid)
}

fn from_sqlite_u64(value: i64, field: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptData { field })
}
