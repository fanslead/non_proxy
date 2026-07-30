use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressFamily {
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceCandidate {
    pub index: NonZeroU32,
    pub operational: bool,
    pub hardware: bool,
    pub filter: bool,
    pub connector_present: bool,
    pub media_connected: bool,
    pub endpoint: bool,
    pub interface_type: u32,
    pub transmit_link_speed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultRouteCandidate {
    pub interface_index: NonZeroU32,
    pub family: AddressFamily,
    pub metric: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PhysicalInterfaces {
    ipv4: Option<NonZeroU32>,
    ipv6: Option<NonZeroU32>,
}

impl PhysicalInterfaces {
    #[must_use]
    pub const fn new(ipv4: Option<NonZeroU32>, ipv6: Option<NonZeroU32>) -> Self {
        Self { ipv4, ipv6 }
    }

    #[must_use]
    pub const fn ipv4(self) -> Option<NonZeroU32> {
        self.ipv4
    }

    #[must_use]
    pub const fn ipv6(self) -> Option<NonZeroU32> {
        self.ipv6
    }

    #[must_use]
    pub const fn for_family(self, family: AddressFamily) -> Option<NonZeroU32> {
        match family {
            AddressFamily::Ipv4 => self.ipv4,
            AddressFamily::Ipv6 => self.ipv6,
        }
    }
}

pub fn select_physical_interfaces(
    interfaces: &[InterfaceCandidate],
    routes: &[DefaultRouteCandidate],
) -> PhysicalInterfaces {
    PhysicalInterfaces::new(
        select_family(interfaces, routes, AddressFamily::Ipv4),
        select_family(interfaces, routes, AddressFamily::Ipv6),
    )
}

fn select_family(
    interfaces: &[InterfaceCandidate],
    routes: &[DefaultRouteCandidate],
    family: AddressFamily,
) -> Option<NonZeroU32> {
    routes
        .iter()
        .filter(|route| route.family == family)
        .filter_map(|route| {
            let interface = interfaces
                .iter()
                .find(|interface| interface.index == route.interface_index)?;
            is_physical(interface).then_some((
                u8::from(interface.connector_present),
                std::cmp::Reverse(route.metric),
                interface.transmit_link_speed,
                std::cmp::Reverse(interface.index.get()),
                interface.index,
            ))
        })
        .max()
        .map(|candidate| candidate.4)
}

fn is_physical(interface: &InterfaceCandidate) -> bool {
    const IF_TYPE_PPP: u32 = 23;
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    const IF_TYPE_TUNNEL: u32 = 131;

    interface.operational
        && interface.hardware
        && !interface.filter
        && interface.media_connected
        && !interface.endpoint
        && !matches!(
            interface.interface_type,
            IF_TYPE_PPP | IF_TYPE_SOFTWARE_LOOPBACK | IF_TYPE_TUNNEL
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
    }

    fn physical(value: u32, connector_present: bool, speed: u64) -> InterfaceCandidate {
        InterfaceCandidate {
            index: index(value),
            operational: true,
            hardware: true,
            filter: false,
            connector_present,
            media_connected: true,
            endpoint: false,
            interface_type: 6,
            transmit_link_speed: speed,
        }
    }

    #[test]
    fn selects_each_address_family_independently() {
        let interfaces = [physical(7, true, 1_000), physical(9, true, 2_000)];
        let routes = [
            DefaultRouteCandidate {
                interface_index: index(7),
                family: AddressFamily::Ipv4,
                metric: 10,
            },
            DefaultRouteCandidate {
                interface_index: index(9),
                family: AddressFamily::Ipv6,
                metric: 20,
            },
        ];

        assert_eq!(
            select_physical_interfaces(&interfaces, &routes),
            PhysicalInterfaces::new(Some(index(7)), Some(index(9)))
        );
    }

    #[test]
    fn rejects_virtual_tunnel_filter_and_disconnected_interfaces() {
        let mut tunnel = physical(1, true, 10_000);
        tunnel.interface_type = 131;
        let mut filter = physical(2, true, 10_000);
        filter.filter = true;
        let mut disconnected = physical(3, true, 10_000);
        disconnected.media_connected = false;
        let routes = [tunnel, filter, disconnected].map(|interface| DefaultRouteCandidate {
            interface_index: interface.index,
            family: AddressFamily::Ipv4,
            metric: 1,
        });

        assert_eq!(
            select_physical_interfaces(&[tunnel, filter, disconnected], &routes),
            PhysicalInterfaces::default()
        );
    }

    #[test]
    fn prefers_connector_then_metric_speed_and_stable_index() {
        let interfaces = [
            physical(8, false, 100_000),
            physical(6, true, 1_000),
            physical(4, true, 1_000),
        ];
        let routes = [
            DefaultRouteCandidate {
                interface_index: index(8),
                family: AddressFamily::Ipv4,
                metric: 1,
            },
            DefaultRouteCandidate {
                interface_index: index(6),
                family: AddressFamily::Ipv4,
                metric: 20,
            },
            DefaultRouteCandidate {
                interface_index: index(4),
                family: AddressFamily::Ipv4,
                metric: 20,
            },
        ];

        assert_eq!(
            select_physical_interfaces(&interfaces, &routes).ipv4(),
            Some(index(4))
        );
    }
}
