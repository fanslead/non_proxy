use std::ptr;

use windows_sys::{
    Win32::NetworkManagement::WindowsFilteringPlatform::{
        FWP_ACTION_CALLOUT_INSPECTION, FWP_ACTION_CALLOUT_TERMINATING, FWP_CONDITION_VALUE0,
        FWP_CONDITION_VALUE0_0, FWP_DIRECTION_OUTBOUND, FWP_MATCH_EQUAL, FWP_MATCH_NOT_EQUAL,
        FWP_UINT8, FWP_UINT16, FWP_UINT32, FWP_VALUE0, FWP_VALUE0_0, FWPM_ACTION0, FWPM_ACTION0_0,
        FWPM_CALLOUT0, FWPM_CONDITION_DIRECTION, FWPM_CONDITION_IP_PROTOCOL,
        FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0, FWPM_FILTER_CONDITION0, FWPM_FILTER0,
        FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4, FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
        FWPM_LAYER_DATAGRAM_DATA_V4, FWPM_LAYER_DATAGRAM_DATA_V6, FwpmCalloutAdd0, FwpmFilterAdd0,
    },
    core::GUID,
};

use crate::WindowsWfpError;

use super::SUBLAYER_KEY;

const UDP_FLOW_CALLOUT_V4_KEY: GUID = GUID::from_u128(0x6a9c6933_d8b0_4cb4_a9d5_e10c4fd15170);
const UDP_FLOW_CALLOUT_V6_KEY: GUID = GUID::from_u128(0x0f985ab5_24c6_48a1_978a_d851f913e421);
const UDP_DATAGRAM_CALLOUT_V4_KEY: GUID = GUID::from_u128(0xc89549f7_2e03_4c6d_88f0_32177bd7d42b);
const UDP_DATAGRAM_CALLOUT_V6_KEY: GUID = GUID::from_u128(0xf08e9b71_a1cc_4ab2_b7e4_52a8b7489605);
const UDP_FLOW_FILTER_V4_KEY: GUID = GUID::from_u128(0x357474e3_69f9_48d2_ae2b_edf77163912d);
const UDP_FLOW_FILTER_V6_KEY: GUID = GUID::from_u128(0x9f75b73d_6f77_496e_95c5_f2a671ff03a0);
const UDP_DATAGRAM_FILTER_V4_KEY: GUID = GUID::from_u128(0x29865491_360b_41f5_9e9e_8d30c91c5c0f);
const UDP_DATAGRAM_FILTER_V6_KEY: GUID = GUID::from_u128(0x61205558_4577_4a86_9c89_66577a191f51);
const DNS_PORT: u16 = 53;

pub(super) fn install(
    engine: *mut core::ffi::c_void,
    provider_key: &mut GUID,
) -> Result<(), WindowsWfpError> {
    for (key, layer, label) in [
        (
            UDP_FLOW_CALLOUT_V4_KEY,
            FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4,
            "NonProxy UDP IPv4 flow identity",
        ),
        (
            UDP_FLOW_CALLOUT_V6_KEY,
            FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
            "NonProxy UDP IPv6 flow identity",
        ),
        (
            UDP_DATAGRAM_CALLOUT_V4_KEY,
            FWPM_LAYER_DATAGRAM_DATA_V4,
            "NonProxy UDP IPv4 datagram divert",
        ),
        (
            UDP_DATAGRAM_CALLOUT_V6_KEY,
            FWPM_LAYER_DATAGRAM_DATA_V6,
            "NonProxy UDP IPv6 datagram divert",
        ),
    ] {
        add_callout(engine, provider_key, key, layer, label)?;
    }
    add_flow_filter(
        engine,
        provider_key,
        UDP_FLOW_FILTER_V4_KEY,
        FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4,
        UDP_FLOW_CALLOUT_V4_KEY,
        "NonProxy UDP IPv4 flow association",
    )?;
    add_flow_filter(
        engine,
        provider_key,
        UDP_FLOW_FILTER_V6_KEY,
        FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
        UDP_FLOW_CALLOUT_V6_KEY,
        "NonProxy UDP IPv6 flow association",
    )?;
    add_datagram_filter(
        engine,
        provider_key,
        UDP_DATAGRAM_FILTER_V4_KEY,
        FWPM_LAYER_DATAGRAM_DATA_V4,
        UDP_DATAGRAM_CALLOUT_V4_KEY,
        "NonProxy UDP IPv4 capture",
    )?;
    add_datagram_filter(
        engine,
        provider_key,
        UDP_DATAGRAM_FILTER_V6_KEY,
        FWPM_LAYER_DATAGRAM_DATA_V6,
        UDP_DATAGRAM_CALLOUT_V6_KEY,
        "NonProxy UDP IPv6 capture",
    )
}

fn add_callout(
    engine: *mut core::ffi::c_void,
    provider_key: &mut GUID,
    key: GUID,
    layer: GUID,
    label: &str,
) -> Result<(), WindowsWfpError> {
    let mut name = wide(label);
    let mut description = wide("只关联身份或搬运 UDP 数据报，不在内核执行产品策略");
    let callout = FWPM_CALLOUT0 {
        calloutKey: key,
        displayData: display(&mut name, &mut description),
        providerKey: provider_key,
        applicableLayer: layer,
        ..Default::default()
    };
    let mut identifier = 0;
    // SAFETY: callout、provider key 与 UTF-16 字符串在同步调用期间有效。
    check("添加 UDP WFP callout", unsafe {
        FwpmCalloutAdd0(engine, &callout, ptr::null_mut(), &mut identifier)
    })
}

fn add_flow_filter(
    engine: *mut core::ffi::c_void,
    provider_key: &mut GUID,
    key: GUID,
    layer: GUID,
    callout: GUID,
    label: &str,
) -> Result<(), WindowsWfpError> {
    let mut conditions = [protocol_condition()];
    add_filter(
        engine,
        provider_key,
        key,
        layer,
        callout,
        label,
        FWP_ACTION_CALLOUT_INSPECTION,
        &mut conditions,
    )
}

fn add_datagram_filter(
    engine: *mut core::ffi::c_void,
    provider_key: &mut GUID,
    key: GUID,
    layer: GUID,
    callout: GUID,
    label: &str,
) -> Result<(), WindowsWfpError> {
    let mut conditions = [
        protocol_condition(),
        FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_DIRECTION,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: FWP_CONDITION_VALUE0 {
                r#type: FWP_UINT32,
                Anonymous: FWP_CONDITION_VALUE0_0 {
                    uint32: FWP_DIRECTION_OUTBOUND as u32,
                },
            },
        },
        FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_IP_REMOTE_PORT,
            matchType: FWP_MATCH_NOT_EQUAL,
            conditionValue: FWP_CONDITION_VALUE0 {
                r#type: FWP_UINT16,
                Anonymous: FWP_CONDITION_VALUE0_0 { uint16: DNS_PORT },
            },
        },
    ];
    add_filter(
        engine,
        provider_key,
        key,
        layer,
        callout,
        label,
        FWP_ACTION_CALLOUT_TERMINATING,
        &mut conditions,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_filter(
    engine: *mut core::ffi::c_void,
    provider_key: &mut GUID,
    key: GUID,
    layer: GUID,
    callout: GUID,
    label: &str,
    action_type: u32,
    conditions: &mut [FWPM_FILTER_CONDITION0],
) -> Result<(), WindowsWfpError> {
    let mut name = wide(label);
    let mut description = wide("排除 DNS 后把通用 UDP 数据报交给受限 Service");
    let filter = FWPM_FILTER0 {
        filterKey: key,
        displayData: display(&mut name, &mut description),
        providerKey: provider_key,
        layerKey: layer,
        subLayerKey: SUBLAYER_KEY,
        weight: FWP_VALUE0 {
            r#type: FWP_UINT8,
            Anonymous: FWP_VALUE0_0 { uint8: 0xa0 },
        },
        numFilterConditions: u32::try_from(conditions.len())
            .map_err(|_| WindowsWfpError::InvalidData("UDP WFP filter 条件数量溢出"))?,
        filterCondition: conditions.as_mut_ptr(),
        action: FWPM_ACTION0 {
            r#type: action_type,
            Anonymous: FWPM_ACTION0_0 {
                calloutKey: callout,
            },
        },
        ..Default::default()
    };
    let mut identifier = 0;
    // SAFETY: filter、conditions、provider key 与字符串在同步调用期间有效。
    check("添加 UDP WFP filter", unsafe {
        FwpmFilterAdd0(engine, &filter, ptr::null_mut(), &mut identifier)
    })
}

fn protocol_condition() -> FWPM_FILTER_CONDITION0 {
    FWPM_FILTER_CONDITION0 {
        fieldKey: FWPM_CONDITION_IP_PROTOCOL,
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 {
            r#type: FWP_UINT8,
            Anonymous: FWP_CONDITION_VALUE0_0 { uint8: 17 },
        },
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn display(name: &mut [u16], description: &mut [u16]) -> FWPM_DISPLAY_DATA0 {
    FWPM_DISPLAY_DATA0 {
        name: name.as_mut_ptr(),
        description: description.as_mut_ptr(),
    }
}

fn check(operation: &'static str, code: u32) -> Result<(), WindowsWfpError> {
    if code == 0 {
        Ok(())
    } else {
        Err(WindowsWfpError::windows(operation, code))
    }
}
