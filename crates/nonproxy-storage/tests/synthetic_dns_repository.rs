use std::{
    error::Error,
    net::{IpAddr, Ipv6Addr},
};

use nonproxy_dns::{SyntheticAddressFamily, SyntheticAddressSpace};
use nonproxy_model::DomainName;
use nonproxy_storage::{PolicyDatabase, SYNTHETIC_BINDING_RETENTION_MS};

fn proposed_prefix() -> Ipv6Addr {
    "fd42:4e50:5258:5901::"
        .parse()
        .unwrap_or(Ipv6Addr::LOCALHOST)
}

#[test]
fn config_and_bindings_survive_repository_reopen() -> Result<(), Box<dyn Error>> {
    let mut database = PolicyDatabase::open_in_memory(1_000)?;
    let space = database
        .synthetic_dns()
        .load_or_create_space(proposed_prefix(), 1_000)?;
    let different_proposal = "fd99:1111:2222:3333::"
        .parse()
        .unwrap_or(Ipv6Addr::LOCALHOST);
    let reopened_space = database
        .synthetic_dns()
        .load_or_create_space(different_proposal, 2_000)?;
    assert_eq!(space, reopened_space);

    let domain = DomainName::normalize("api.example")?;
    let ipv4 = database.synthetic_dns().get_or_create(
        space,
        &domain,
        SyntheticAddressFamily::Ipv4,
        3_000,
    )?;
    let repeated = database.synthetic_dns().get_or_create(
        space,
        &domain,
        SyntheticAddressFamily::Ipv4,
        4_000,
    )?;
    let ipv6 = database.synthetic_dns().get_or_create(
        space,
        &domain,
        SyntheticAddressFamily::Ipv6,
        4_000,
    )?;

    assert_eq!(ipv4.address(), repeated.address());
    assert!(ipv4.address().is_ipv4());
    assert!(ipv6.address().is_ipv6());
    assert_eq!(
        repeated.retain_until_unix_ms(),
        4_000 + SYNTHETIC_BINDING_RETENTION_MS
    );
    assert_eq!(
        database
            .synthetic_dns()
            .lookup(space, ipv4.address(), 5_000)?
            .map(|binding| binding.domain().clone()),
        Some(domain)
    );
    assert_eq!(
        database
            .synthetic_dns()
            .list_retained(space, 5_000, 8)?
            .len(),
        2
    );
    Ok(())
}

#[test]
fn collision_uses_the_next_free_address_without_changing_existing_binding()
-> Result<(), Box<dyn Error>> {
    let mut database = PolicyDatabase::open_in_memory(1_000)?;
    let space = database
        .synthetic_dns()
        .load_or_create_space(proposed_prefix(), 1_000)?;
    let (first_domain, second_domain) = colliding_domains(space)?;
    let first = database.synthetic_dns().get_or_create(
        space,
        &first_domain,
        SyntheticAddressFamily::Ipv4,
        2_000,
    )?;
    let second = database.synthetic_dns().get_or_create(
        space,
        &second_domain,
        SyntheticAddressFamily::Ipv4,
        2_000,
    )?;
    let repeated = database.synthetic_dns().get_or_create(
        space,
        &first_domain,
        SyntheticAddressFamily::Ipv4,
        3_000,
    )?;

    assert_ne!(first.address(), second.address());
    assert_eq!(first.address(), repeated.address());
    Ok(())
}

fn colliding_domains(
    space: SyntheticAddressSpace,
) -> Result<(DomainName, DomainName), Box<dyn Error>> {
    let mut seen = std::collections::HashMap::<IpAddr, DomainName>::new();
    for index in 0..10_000_u32 {
        let domain = DomainName::normalize(&format!("host-{index}.collision.example"))?;
        let address = space.candidate(&domain, SyntheticAddressFamily::Ipv4, 0)?;
        if let Some(first) = seen.insert(address, domain.clone()) {
            return Ok((first, domain));
        }
    }
    Err("测试范围内未找到散列碰撞".into())
}
