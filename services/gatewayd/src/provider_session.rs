use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use nonproxy_proto::provider::v1::ProviderRequestContext;
use tonic::Status;

use crate::GatewayError;

const SESSION_TOKEN_LENGTH: usize = 32;
const STARTUP_NONCE_LENGTH: usize = 32;
const MAX_INSTANCE_ID_LENGTH: usize = 128;
const MAX_ACTIVE_SESSIONS: usize = 64;
const NONCE_HISTORY_CAPACITY: usize = 1_024;
pub const PROVIDER_SESSION_LIFETIME_MS: u64 = 15 * 60 * 1_000;

#[derive(Clone)]
pub struct ProviderSessionRegistry {
    state: Arc<Mutex<ProviderSessionState>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSessionHandle {
    provider_id: String,
    generation: u64,
}

struct ProviderSessionState {
    sessions: HashMap<String, ProviderSession>,
    used_nonces: HashSet<[u8; STARTUP_NONCE_LENGTH]>,
    nonce_order: VecDeque<[u8; STARTUP_NONCE_LENGTH]>,
}

struct ProviderSession {
    provider_id: String,
    generation: u64,
    token: [u8; SESSION_TOKEN_LENGTH],
    expires_at_unix_ms: u64,
    last_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredProviderSession {
    token: [u8; SESSION_TOKEN_LENGTH],
    expires_at_unix_ms: u64,
}

impl ProviderSessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ProviderSessionState {
                sessions: HashMap::new(),
                used_nonces: HashSet::new(),
                nonce_order: VecDeque::new(),
            })),
        }
    }

    pub fn register(
        &self,
        instance_id: String,
        provider_id: String,
        generation: u64,
        startup_nonce: &[u8],
        now_unix_ms: u64,
    ) -> Result<RegisteredProviderSession, GatewayError> {
        validate_registration_input(&instance_id, startup_nonce)?;
        let nonce: [u8; STARTUP_NONCE_LENGTH] = startup_nonce
            .try_into()
            .map_err(|_| GatewayError::InvalidRequest("startup_nonce 必须为 32 字节"))?;
        let mut token = [0_u8; SESSION_TOKEN_LENGTH];
        getrandom::fill(&mut token).map_err(|error| GatewayError::Random(error.to_string()))?;
        let expires_at_unix_ms = now_unix_ms
            .checked_add(PROVIDER_SESSION_LIFETIME_MS)
            .ok_or(GatewayError::ClockOverflow)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("Provider 会话"))?;
        if state.used_nonces.contains(&nonce) {
            return Err(GatewayError::InvalidRequest("startup_nonce 已经使用"));
        }
        state
            .sessions
            .retain(|_, session| session.expires_at_unix_ms > now_unix_ms);
        if !state.sessions.contains_key(&instance_id) && state.sessions.len() >= MAX_ACTIVE_SESSIONS
        {
            return Err(GatewayError::InvalidRequest(
                "Provider 活跃会话数量超过上限",
            ));
        }
        remember_nonce(&mut state, nonce);
        state.sessions.insert(
            instance_id,
            ProviderSession {
                provider_id,
                generation,
                token,
                expires_at_unix_ms,
                last_sequence: 0,
            },
        );
        Ok(RegisteredProviderSession {
            token,
            expires_at_unix_ms,
        })
    }

    pub fn validate(
        &self,
        context: Option<&ProviderRequestContext>,
        now_unix_ms: u64,
    ) -> Result<ProviderSessionHandle, Status> {
        let context = context.ok_or_else(|| Status::unauthenticated("缺少 Provider 会话上下文"))?;
        validate_instance_id(&context.provider_instance_id)
            .map_err(|_| Status::invalid_argument("provider_instance_id 无效"))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::unavailable("Provider 会话状态不可用"))?;
        let session = state
            .sessions
            .get_mut(&context.provider_instance_id)
            .ok_or_else(|| Status::unauthenticated("Provider 会话不存在或已重启"))?;
        if now_unix_ms >= session.expires_at_unix_ms {
            return Err(Status::unauthenticated("Provider 会话已经过期"));
        }
        if !constant_time_equal(&session.token, &context.session_token) {
            return Err(Status::permission_denied("Provider 会话令牌无效"));
        }
        if context.request_sequence == 0 || context.request_sequence <= session.last_sequence {
            return Err(Status::permission_denied("Provider 请求序列重放"));
        }
        session.last_sequence = context.request_sequence;
        Ok(ProviderSessionHandle {
            provider_id: session.provider_id.clone(),
            generation: session.generation,
        })
    }
}

impl Default for ProviderSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderSessionHandle {
    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

impl RegisteredProviderSession {
    #[must_use]
    pub fn token(&self) -> &[u8] {
        &self.token
    }

    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }
}

pub(crate) fn validate_registration_input(
    instance_id: &str,
    startup_nonce: &[u8],
) -> Result<(), GatewayError> {
    validate_instance_id(instance_id)?;
    if startup_nonce.len() != STARTUP_NONCE_LENGTH {
        return Err(GatewayError::InvalidRequest("startup_nonce 必须为 32 字节"));
    }
    Ok(())
}

fn validate_instance_id(value: &str) -> Result<(), GatewayError> {
    if value.is_empty()
        || value.len() > MAX_INSTANCE_ID_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(GatewayError::InvalidRequest("provider_instance_id 无效"));
    }
    Ok(())
}

fn remember_nonce(state: &mut ProviderSessionState, nonce: [u8; STARTUP_NONCE_LENGTH]) {
    if state.nonce_order.len() == NONCE_HISTORY_CAPACITY
        && let Some(expired) = state.nonce_order.pop_front()
    {
        state.used_nonces.remove(&expired);
    }
    state.used_nonces.insert(nonce);
    state.nonce_order.push_back(nonce);
}

fn constant_time_equal(expected: &[u8; SESSION_TOKEN_LENGTH], actual: &[u8]) -> bool {
    if actual.len() != SESSION_TOKEN_LENGTH {
        return false;
    }
    let difference = expected
        .iter()
        .zip(actual)
        .fold(0_u8, |value, (left, right)| value | (left ^ right));
    difference == 0
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::provider::v1::ProviderRequestContext;

    use super::{MAX_ACTIVE_SESSIONS, ProviderSessionRegistry};

    #[test]
    fn rejects_replayed_sequence_and_startup_nonce() {
        let registry = ProviderSessionRegistry::new();
        let registered = registry.register(
            "transparent-1".to_owned(),
            "transparent-proxy".to_owned(),
            4,
            &[1; 32],
            1_000,
        );
        let Ok(registered) = registered else {
            panic!("Provider 测试会话注册失败: {registered:?}");
        };
        let context = ProviderRequestContext {
            provider_instance_id: "transparent-1".to_owned(),
            session_token: registered.token().to_vec(),
            request_sequence: 1,
        };

        assert!(registry.validate(Some(&context), 1_100).is_ok());
        assert!(registry.validate(Some(&context), 1_200).is_err());
        assert!(
            registry
                .register(
                    "transparent-2".to_owned(),
                    "transparent-proxy".to_owned(),
                    5,
                    &[1; 32],
                    1_300,
                )
                .is_err()
        );
    }

    #[test]
    fn bounds_active_sessions_and_removes_expired_entries() {
        let registry = ProviderSessionRegistry::new();
        for index in 0..MAX_ACTIVE_SESSIONS {
            let result = registry.register(
                format!("provider-{index}"),
                "transparent-proxy".to_owned(),
                index as u64 + 1,
                &[index as u8; 32],
                1_000,
            );
            assert!(result.is_ok());
        }
        assert!(
            registry
                .register(
                    "provider-overflow".to_owned(),
                    "transparent-proxy".to_owned(),
                    100,
                    &[100; 32],
                    1_000,
                )
                .is_err()
        );
        assert!(
            registry
                .register(
                    "provider-after-expiry".to_owned(),
                    "transparent-proxy".to_owned(),
                    101,
                    &[101; 32],
                    901_000,
                )
                .is_ok()
        );
    }
}
