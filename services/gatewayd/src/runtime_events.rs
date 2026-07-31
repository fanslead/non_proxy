use std::{collections::HashMap, sync::Arc};

use nonproxy_proto::{
    common::v1::{ComponentKind, ErrorDetail, Severity},
    events::v1::{
        ComponentHealthChanged, EventEnvelope, RuntimeState, SystemStateChanged, event_envelope,
    },
};
use tokio::sync::Mutex;

use crate::{Gateway, GatewayError, event_hub::EventHub, provider_health::ComponentHealthSnapshot};

#[derive(Clone, Debug, PartialEq)]
pub struct SystemRuntimeSnapshot {
    state: RuntimeState,
    active_snapshot_version: u64,
    data_plane_enabled: bool,
    error: Option<ErrorDetail>,
}

impl SystemRuntimeSnapshot {
    #[must_use]
    pub fn new(
        state: RuntimeState,
        active_snapshot_version: u64,
        data_plane_enabled: bool,
    ) -> Self {
        let error = (state == RuntimeState::Degraded).then(|| ErrorDetail {
            code: "NP_DATA_PLANE_NOT_READY".to_owned(),
            message: "策略控制面可用，但数据面尚未全部就绪。".to_owned(),
            retryable: true,
            metadata: Default::default(),
        });
        Self {
            state,
            active_snapshot_version,
            data_plane_enabled,
            error,
        }
    }

    #[must_use]
    pub const fn state(&self) -> RuntimeState {
        self.state
    }

    #[must_use]
    pub const fn active_snapshot_version(&self) -> u64 {
        self.active_snapshot_version
    }

    #[must_use]
    pub const fn data_plane_enabled(&self) -> bool {
        self.data_plane_enabled
    }

    #[must_use]
    pub const fn error(&self) -> Option<&ErrorDetail> {
        self.error.as_ref()
    }
}

#[derive(Clone, Default)]
pub struct RuntimeEventPublisher {
    state: Arc<Mutex<PublishedRuntimeState>>,
}

#[derive(Default)]
struct PublishedRuntimeState {
    components: HashMap<i32, ComponentFingerprint>,
    system: Option<SystemFingerprint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ComponentFingerprint {
    state: i32,
    active_snapshot_version: u64,
    error_code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SystemFingerprint {
    state: i32,
    active_snapshot_version: u64,
    data_plane_enabled: bool,
    error_code: String,
}

impl RuntimeEventPublisher {
    pub async fn publish(&self, gateway: &Gateway, now_unix_ms: u64) -> Result<(), GatewayError> {
        // 串行化“读取权威状态 + 发布”，避免较早读取的并发心跳在较新事件之后倒序广播。
        let mut published = self.state.lock().await;
        let (_status, system, components) = gateway.runtime_status_at(now_unix_ms).await?;
        Self::publish_snapshots(&mut published, gateway.events(), &system, &components)
    }

    fn publish_snapshots(
        published: &mut PublishedRuntimeState,
        events: &EventHub,
        system: &SystemRuntimeSnapshot,
        components: &[ComponentHealthSnapshot],
    ) -> Result<(), GatewayError> {
        for component in components {
            let fingerprint = ComponentFingerprint::from(component);
            let key = component.component() as i32;
            if published.components.get(&key) == Some(&fingerprint) {
                continue;
            }
            events.publish(component_event(component))?;
            published.components.insert(key, fingerprint);
        }
        let fingerprint = SystemFingerprint::from(system);
        if published.system.as_ref() != Some(&fingerprint) {
            events.publish(system_event(system))?;
            published.system = Some(fingerprint);
        }
        Ok(())
    }
}

impl From<&ComponentHealthSnapshot> for ComponentFingerprint {
    fn from(value: &ComponentHealthSnapshot) -> Self {
        Self {
            state: value.state() as i32,
            active_snapshot_version: value.active_snapshot_version(),
            error_code: value
                .error()
                .map_or_else(String::new, |error| error.code.clone()),
        }
    }
}

impl From<&SystemRuntimeSnapshot> for SystemFingerprint {
    fn from(value: &SystemRuntimeSnapshot) -> Self {
        Self {
            state: value.state() as i32,
            active_snapshot_version: value.active_snapshot_version(),
            data_plane_enabled: value.data_plane_enabled(),
            error_code: value
                .error()
                .map_or_else(String::new, |error| error.code.clone()),
        }
    }
}

fn component_event(value: &ComponentHealthSnapshot) -> EventEnvelope {
    let error = value.error().cloned();
    EventEnvelope {
        component: value.component() as i32,
        severity: severity(value.state()) as i32,
        error_code: error
            .as_ref()
            .map_or_else(String::new, |detail| detail.code.clone()),
        snapshot_version: value.active_snapshot_version(),
        payload: Some(event_envelope::Payload::ComponentHealthChanged(
            ComponentHealthChanged {
                component: value.component() as i32,
                state: value.state() as i32,
                error,
            },
        )),
        ..Default::default()
    }
}

fn system_event(value: &SystemRuntimeSnapshot) -> EventEnvelope {
    let error = value.error().cloned();
    EventEnvelope {
        component: ComponentKind::Gateway as i32,
        severity: severity(value.state()) as i32,
        error_code: error
            .as_ref()
            .map_or_else(String::new, |detail| detail.code.clone()),
        snapshot_version: value.active_snapshot_version(),
        payload: Some(event_envelope::Payload::SystemStateChanged(
            SystemStateChanged {
                state: value.state() as i32,
                active_snapshot_version: value.active_snapshot_version(),
                data_plane_enabled: value.data_plane_enabled(),
                error,
            },
        )),
        ..Default::default()
    }
}

const fn severity(state: RuntimeState) -> Severity {
    match state {
        RuntimeState::Failed => Severity::Error,
        RuntimeState::Degraded | RuntimeState::Draining => Severity::Warning,
        RuntimeState::Stopped | RuntimeState::Starting | RuntimeState::Ready => Severity::Info,
        RuntimeState::Unspecified => Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::events::v1::{RuntimeState, event_envelope};
    use nonproxy_storage::PolicyDatabase;

    use super::RuntimeEventPublisher;
    use crate::{Gateway, provider_health::component_for_provider, provider_requirements};

    #[tokio::test]
    async fn publishes_only_semantic_runtime_changes() {
        let database = PolicyDatabase::open_in_memory(1)
            .unwrap_or_else(|error| panic!("运行事件测试数据库失败: {error}"));
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let publisher = RuntimeEventPublisher::default();

        assert!(publisher.publish(&gateway, 1_000).await.is_ok());
        assert!(publisher.publish(&gateway, 2_000).await.is_ok());
        let events = gateway.events().subscribe(0);
        let Ok((events, _receiver)) = events else {
            panic!("运行事件订阅失败: {events:?}");
        };
        let component_count = provider_requirements::required_provider_ids()
            .iter()
            .map(|provider_id| component_for_provider(provider_id) as i32)
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(events.len(), component_count + 1);
        assert!(events.iter().any(|event| {
            matches!(
                event.payload.as_ref(),
                Some(event_envelope::Payload::SystemStateChanged(value))
                    if value.state == RuntimeState::Stopped as i32
            )
        }));

        for provider_id in provider_requirements::required_provider_ids() {
            assert!(
                gateway
                    .mark_provider_registered(provider_id, 1, 3_000)
                    .is_ok()
            );
        }
        assert!(publisher.publish(&gateway, 3_000).await.is_ok());
        assert!(publisher.publish(&gateway, 3_001).await.is_ok());
        let events = gateway.events().subscribe(0);
        let Ok((events, _receiver)) = events else {
            panic!("运行事件二次订阅失败: {events:?}");
        };
        assert_eq!(events.len(), (component_count * 2) + 1);
        let first_component =
            component_for_provider(provider_requirements::required_provider_ids()[0]);
        assert!(events.iter().any(|event| {
            event.component == first_component as i32
                && matches!(
                    event.payload.as_ref(),
                    Some(event_envelope::Payload::ComponentHealthChanged(value))
                        if value.error.is_none()
                )
        }));
    }

    #[tokio::test]
    async fn heartbeat_expiry_and_recovery_each_publish_once() {
        let database = PolicyDatabase::open_in_memory(1)
            .unwrap_or_else(|error| panic!("心跳事件测试数据库失败: {error}"));
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let publisher = RuntimeEventPublisher::default();
        for provider_id in provider_requirements::required_provider_ids() {
            assert!(
                gateway
                    .report_provider_health(provider_id, 1, RuntimeState::Ready, 0, 1_000,)
                    .is_ok()
            );
        }
        assert!(publisher.publish(&gateway, 1_000).await.is_ok());
        assert!(publisher.publish(&gateway, 16_001).await.is_ok());
        assert!(publisher.publish(&gateway, 20_000).await.is_ok());
        let first_provider = provider_requirements::required_provider_ids()[0];
        for provider_id in provider_requirements::required_provider_ids() {
            assert!(
                gateway
                    .report_provider_health(provider_id, 1, RuntimeState::Ready, 0, 20_001,)
                    .is_ok()
            );
        }
        assert!(publisher.publish(&gateway, 20_001).await.is_ok());

        let events = gateway.events().subscribe(0);
        let Ok((events, _receiver)) = events else {
            panic!("心跳事件订阅失败: {events:?}");
        };
        let component = component_for_provider(first_provider);
        let component_events = events
            .iter()
            .filter(|event| event.component == component as i32)
            .collect::<Vec<_>>();
        assert_eq!(component_events.len(), 3);
        assert!(matches!(
            component_events[1].payload.as_ref(),
            Some(event_envelope::Payload::ComponentHealthChanged(value))
                if value.state == RuntimeState::Degraded as i32
                    && value.error.as_ref().is_some_and(|error| {
                        error.code == "NP_PROVIDER_HEARTBEAT_STALE"
                    })
        ));
        assert!(matches!(
            component_events[2].payload.as_ref(),
            Some(event_envelope::Payload::ComponentHealthChanged(value))
                if value.state == RuntimeState::Ready as i32
        ));
    }
}
