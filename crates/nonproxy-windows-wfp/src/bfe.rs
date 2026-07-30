use std::ptr;

use windows_sys::{
    Win32::{
        Foundation::HANDLE,
        NetworkManagement::WindowsFilteringPlatform::{
            FWP_ACTION_CALLOUT_TERMINATING, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0,
            FWP_MATCH_EQUAL, FWP_UINT8, FWP_UINT16, FWP_VALUE0, FWP_VALUE0_0, FWPM_ACTION0,
            FWPM_ACTION0_0, FWPM_CALLOUT0, FWPM_CONDITION_IP_PROTOCOL,
            FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0, FWPM_FILTER_CONDITION0,
            FWPM_FILTER0, FWPM_FILTER0_0, FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
            FWPM_LAYER_ALE_CONNECT_REDIRECT_V6, FWPM_PROVIDER0, FWPM_SESSION_FLAG_DYNAMIC,
            FWPM_SESSION0, FWPM_SUBLAYER0, FwpmCalloutAdd0, FwpmEngineClose0, FwpmEngineOpen0,
            FwpmFilterAdd0, FwpmProviderAdd0, FwpmSubLayerAdd0, FwpmTransactionAbort0,
            FwpmTransactionBegin0, FwpmTransactionCommit0,
        },
        Networking::WinSock::{IPPROTO_TCP, IPPROTO_UDP},
        System::Rpc::RPC_C_AUTHN_WINNT,
    },
    core::GUID,
};

use crate::WindowsWfpError;

pub const PROVIDER_KEY: GUID = GUID::from_u128(0x40485aa1_1262_4be1_80f8_574ad4d264e5);
pub const SUBLAYER_KEY: GUID = GUID::from_u128(0xd8566362_525d_40de_946f_50ad7239a80e);
pub const CALLOUT_V4_KEY: GUID = GUID::from_u128(0x32715ea8_87fd_4da0_8f7f_2dfbb1f8dbd2);
pub const CALLOUT_V6_KEY: GUID = GUID::from_u128(0xa9fe83c7_813e_4653_a44d_b5a4564fc632);
const FILTER_V4_KEY: GUID = GUID::from_u128(0x496772cb_fa44_47a1_b5cb_0c7598767f9b);
const FILTER_V6_KEY: GUID = GUID::from_u128(0xf4b0c29a_5c56_4b3a_ab41_960c9f7bb3e7);
const DNS_TCP_FILTER_V4_KEY: GUID = GUID::from_u128(0x5ca711c6_43bc_4e65_b8d1_1fa29071614f);
const DNS_TCP_FILTER_V6_KEY: GUID = GUID::from_u128(0xfdb52a0d_dc88_450a_8023_273ace34ca5c);
const DNS_UDP_FILTER_V4_KEY: GUID = GUID::from_u128(0x9289ea15_c752_4729_9763_45778d132bd7);
const DNS_UDP_FILTER_V6_KEY: GUID = GUID::from_u128(0x42fb3193_973a_4f80_8772_e32c010c9b41);
const FILTER_CONTEXT_TCP: u64 = 1;
const FILTER_CONTEXT_DNS: u64 = 2;
const DNS_PORT: u16 = 53;

struct FilterSpec {
    key: GUID,
    layer: GUID,
    callout: GUID,
    label: &'static str,
    protocol: u8,
    remote_port: Option<u16>,
    raw_context: u64,
    weight: u8,
}

pub struct DynamicWfpSession {
    engine: HANDLE,
}

impl DynamicWfpSession {
    pub fn install() -> Result<Self, WindowsWfpError> {
        let mut session_name = wide("NonProxy 动态 WFP 会话");
        let mut session_description = wide("网关退出时由 BFE 自动移除");
        let session = FWPM_SESSION0 {
            displayData: display(&mut session_name, &mut session_description),
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            txnWaitTimeoutInMSec: 5_000,
            ..Default::default()
        };
        let mut engine = ptr::null_mut();
        // SAFETY: session 及输出 handle 在同步调用期间有效；本机 BFE 不需要认证身份。
        check("打开 WFP 引擎", unsafe {
            FwpmEngineOpen0(
                ptr::null(),
                RPC_C_AUTHN_WINNT,
                ptr::null(),
                &session,
                &mut engine,
            )
        })?;
        let mut result = Self { engine };
        if let Err(error) = result.install_transaction() {
            let _close_result = result.close();
            return Err(error);
        }
        Ok(result)
    }

    fn install_transaction(&mut self) -> Result<(), WindowsWfpError> {
        check(
            "开始 WFP 安装事务",
            // SAFETY: engine 是成功打开且尚未关闭的 BFE handle。
            unsafe { FwpmTransactionBegin0(self.engine, 0) },
        )?;
        let result = self.install_objects();
        if let Err(error) = result {
            // SAFETY: 当前 handle 上存在尚未提交的事务。
            let _abort_result = unsafe { FwpmTransactionAbort0(self.engine) };
            return Err(error);
        }
        check(
            "提交 WFP 安装事务",
            // SAFETY: 当前 handle 上存在完整事务，提交后对象归动态 session 所有。
            unsafe { FwpmTransactionCommit0(self.engine) },
        )
    }

    fn install_objects(&self) -> Result<(), WindowsWfpError> {
        let mut provider_key = PROVIDER_KEY;
        let mut provider_name = wide("NonProxy");
        let mut provider_description = wide("NonProxy TCP 与明文 DNS connect redirect provider");
        let provider = FWPM_PROVIDER0 {
            providerKey: provider_key,
            displayData: display(&mut provider_name, &mut provider_description),
            ..Default::default()
        };
        check(
            "添加 WFP provider",
            // SAFETY: 所有结构和字符串在同步调用期间有效。
            unsafe { FwpmProviderAdd0(self.engine, &provider, ptr::null_mut()) },
        )?;

        let mut sublayer_name = wide("NonProxy connect redirect");
        let mut sublayer_description = wide("处理 TCP 与远端 53 端口 DNS");
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: SUBLAYER_KEY,
            displayData: display(&mut sublayer_name, &mut sublayer_description),
            providerKey: &mut provider_key,
            weight: 0x7f00,
            ..Default::default()
        };
        check(
            "添加 WFP sublayer",
            // SAFETY: 所有结构和 provider key 在同步调用期间有效。
            unsafe { FwpmSubLayerAdd0(self.engine, &sublayer, ptr::null_mut()) },
        )?;

        self.add_callout(
            &mut provider_key,
            CALLOUT_V4_KEY,
            FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
            "NonProxy TCP IPv4 redirect",
        )?;
        self.add_callout(
            &mut provider_key,
            CALLOUT_V6_KEY,
            FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
            "NonProxy TCP IPv6 redirect",
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: DNS_TCP_FILTER_V4_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
                callout: CALLOUT_V4_KEY,
                label: "NonProxy DNS TCP IPv4 capture",
                protocol: IPPROTO_TCP as u8,
                remote_port: Some(DNS_PORT),
                raw_context: FILTER_CONTEXT_DNS,
                weight: 0xf0,
            },
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: DNS_TCP_FILTER_V6_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
                callout: CALLOUT_V6_KEY,
                label: "NonProxy DNS TCP IPv6 capture",
                protocol: IPPROTO_TCP as u8,
                remote_port: Some(DNS_PORT),
                raw_context: FILTER_CONTEXT_DNS,
                weight: 0xf0,
            },
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: DNS_UDP_FILTER_V4_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
                callout: CALLOUT_V4_KEY,
                label: "NonProxy DNS UDP IPv4 capture",
                protocol: IPPROTO_UDP as u8,
                remote_port: Some(DNS_PORT),
                raw_context: FILTER_CONTEXT_DNS,
                weight: 0xf0,
            },
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: DNS_UDP_FILTER_V6_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
                callout: CALLOUT_V6_KEY,
                label: "NonProxy DNS UDP IPv6 capture",
                protocol: IPPROTO_UDP as u8,
                remote_port: Some(DNS_PORT),
                raw_context: FILTER_CONTEXT_DNS,
                weight: 0xf0,
            },
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: FILTER_V4_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
                callout: CALLOUT_V4_KEY,
                label: "NonProxy TCP IPv4 capture",
                protocol: IPPROTO_TCP as u8,
                remote_port: None,
                raw_context: FILTER_CONTEXT_TCP,
                weight: 0x80,
            },
        )?;
        self.add_filter(
            &mut provider_key,
            FilterSpec {
                key: FILTER_V6_KEY,
                layer: FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
                callout: CALLOUT_V6_KEY,
                label: "NonProxy TCP IPv6 capture",
                protocol: IPPROTO_TCP as u8,
                remote_port: None,
                raw_context: FILTER_CONTEXT_TCP,
                weight: 0x80,
            },
        )
    }

    fn add_callout(
        &self,
        provider_key: &mut GUID,
        callout_key: GUID,
        layer_key: GUID,
        label: &str,
    ) -> Result<(), WindowsWfpError> {
        let mut name = wide(label);
        let mut description = wide("NonProxy 最小内核重定向 callout");
        let callout = FWPM_CALLOUT0 {
            calloutKey: callout_key,
            displayData: display(&mut name, &mut description),
            providerKey: provider_key,
            applicableLayer: layer_key,
            ..Default::default()
        };
        let mut identifier = 0;
        check(
            "添加 WFP callout",
            // SAFETY: callout 结构及其指针在同步调用期间有效。
            unsafe { FwpmCalloutAdd0(self.engine, &callout, ptr::null_mut(), &mut identifier) },
        )
    }

    fn add_filter(&self, provider_key: &mut GUID, spec: FilterSpec) -> Result<(), WindowsWfpError> {
        let mut name = wide(spec.label);
        let mut description = wide("内核只做协议与端口筛选，策略判定留在用户态");
        let mut conditions = vec![FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_IP_PROTOCOL,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: FWP_CONDITION_VALUE0 {
                r#type: FWP_UINT8,
                Anonymous: FWP_CONDITION_VALUE0_0 {
                    uint8: spec.protocol,
                },
            },
        }];
        if let Some(port) = spec.remote_port {
            conditions.push(FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_IP_REMOTE_PORT,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_UINT16,
                    Anonymous: FWP_CONDITION_VALUE0_0 { uint16: port },
                },
            });
        }
        let filter = FWPM_FILTER0 {
            filterKey: spec.key,
            displayData: display(&mut name, &mut description),
            providerKey: provider_key,
            layerKey: spec.layer,
            subLayerKey: SUBLAYER_KEY,
            weight: FWP_VALUE0 {
                r#type: FWP_UINT8,
                Anonymous: FWP_VALUE0_0 { uint8: spec.weight },
            },
            numFilterConditions: u32::try_from(conditions.len())
                .map_err(|_| WindowsWfpError::InvalidData("WFP filter 条件数量溢出"))?,
            filterCondition: conditions.as_mut_ptr(),
            action: FWPM_ACTION0 {
                r#type: FWP_ACTION_CALLOUT_TERMINATING,
                Anonymous: FWPM_ACTION0_0 {
                    calloutKey: spec.callout,
                },
            },
            Anonymous: FWPM_FILTER0_0 {
                rawContext: spec.raw_context,
            },
            ..Default::default()
        };
        let mut identifier = 0;
        check(
            "添加 WFP filter",
            // SAFETY: filter、condition 和字符串在同步调用期间有效。
            unsafe { FwpmFilterAdd0(self.engine, &filter, ptr::null_mut(), &mut identifier) },
        )
    }

    fn close(&mut self) -> Result<(), WindowsWfpError> {
        if self.engine.is_null() {
            return Ok(());
        }
        let engine = std::mem::replace(&mut self.engine, ptr::null_mut());
        check(
            "关闭 WFP 引擎",
            // SAFETY: engine 是本对象唯一持有的有效 BFE handle。
            unsafe { FwpmEngineClose0(engine) },
        )
    }
}

impl Drop for DynamicWfpSession {
    fn drop(&mut self) {
        let _close_result = self.close();
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

// BFE engine handle 可跨线程关闭；所有修改在构造阶段同步完成，之后只承担 RAII 清理。
unsafe impl Send for DynamicWfpSession {}
unsafe impl Sync for DynamicWfpSession {}
