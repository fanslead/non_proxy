use nonproxy_learning::{
    BrowserContextId, ConfirmationId, LearningObservation, LearningObservationKind,
    LearningResourceType, LearningSession, LearningSessionId, LearningSubject, ObservationId,
};
use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, DomainName, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind, Transport,
};
use nonproxy_storage::{LearningPolicySelection, PolicyDatabase, StorageError};

#[test]
fn confirmation_writes_the_selected_policy_batch_and_replays() {
    let mut database = database_with_stopped_session();
    let confirmation_id = confirmation_id("confirmation-a");
    let session_id = session_id();
    let selections = vec![
        selection("www.example.com", "learned-main"),
        selection("api.example.com", "learned-api"),
    ];

    let confirmed = database.learning_confirmations().confirm_site(
        &confirmation_id,
        &session_id,
        &selections,
        4_000,
    );
    let Ok(confirmed) = confirmed else {
        panic!("候选确认失败: {confirmed:?}");
    };
    assert!(!confirmed.replayed());
    assert_eq!(confirmed.policies().len(), 2);
    assert!(matches!(
        database.policies().list(),
        Ok(value) if value.len() == 2
    ));

    let replay = database.learning_confirmations().confirm_site(
        &confirmation_id,
        &session_id,
        &selections,
        5_000,
    );
    assert!(matches!(replay, Ok(value) if value.replayed()));
    assert!(matches!(
        database.policies().list(),
        Ok(value) if value.len() == 2
    ));

    assert!(
        database
            .learning_confirmations()
            .mark_snapshot(&confirmation_id, 7)
            .is_ok()
    );
    let stored = database.learning_confirmations().get(&confirmation_id);
    assert!(matches!(
        stored,
        Ok(Some(value)) if value.snapshot_version() == Some(7)
    ));
}

#[test]
fn confirmation_requires_a_finished_session_and_the_main_site() {
    let mut active_database = database_with_active_session();
    let active_result = active_database.learning_confirmations().confirm_site(
        &confirmation_id("confirmation-active"),
        &session_id(),
        &[selection("www.example.com", "learned-main")],
        2_000,
    );
    assert!(matches!(
        active_result,
        Err(StorageError::LearningSessionStillActive)
    ));

    let mut stopped_database = database_with_stopped_session();
    let missing_main = stopped_database.learning_confirmations().confirm_site(
        &confirmation_id("confirmation-missing-main"),
        &session_id(),
        &[selection("api.example.com", "learned-api")],
        4_000,
    );
    assert!(matches!(
        missing_main,
        Err(StorageError::LearningConfirmationInvalid)
    ));
    assert!(matches!(
        stopped_database.policies().list(),
        Ok(value) if value.is_empty()
    ));
}

#[test]
fn confirmation_rejects_unknown_candidates_and_changed_replays() {
    let mut database = database_with_stopped_session();
    let confirmation = confirmation_id("confirmation-a");
    let session_id = session_id();
    let selections = vec![selection("www.example.com", "learned-main")];
    if let Err(error) = database.learning_confirmations().confirm_site(
        &confirmation,
        &session_id,
        &selections,
        4_000,
    ) {
        panic!("首个确认失败: {error}");
    }

    let changed = database.learning_confirmations().confirm_site(
        &confirmation,
        &session_id,
        &[
            selection("www.example.com", "learned-main"),
            selection("api.example.com", "learned-api"),
        ],
        5_000,
    );
    assert!(matches!(
        changed,
        Err(StorageError::LearningConfirmationReplayMismatch)
    ));

    let mut other_database = database_with_stopped_session();
    let unknown = other_database.learning_confirmations().confirm_site(
        &confirmation_id("confirmation-unknown"),
        &session_id,
        &[
            selection("www.example.com", "learned-main"),
            selection("unknown.example.com", "learned-unknown"),
        ],
        4_000,
    );
    assert!(matches!(
        unknown,
        Err(StorageError::LearningConfirmationInvalid)
    ));
    assert!(matches!(
        other_database.policies().list(),
        Ok(value) if value.is_empty()
    ));
}

#[test]
fn confirmation_rejects_site_rules_with_hidden_transport_scope() {
    let mut database = database_with_stopped_session();
    let result = database.learning_confirmations().confirm_site(
        &confirmation_id("confirmation-scoped"),
        &session_id(),
        &[selection_with_transports(
            "www.example.com",
            "learned-main",
            vec![Transport::Tcp],
        )],
        4_000,
    );

    assert!(matches!(
        result,
        Err(StorageError::LearningConfirmationInvalid)
    ));
    assert!(matches!(
        database.policies().list(),
        Ok(value) if value.is_empty()
    ));
}

fn database_with_stopped_session() -> PolicyDatabase {
    let mut database = database_with_active_session();
    if let Err(error) = database.learning().stop(&session_id(), 3_000) {
        panic!("停止学习会话失败: {error}");
    }
    database
}

fn database_with_active_session() -> PolicyDatabase {
    let mut database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("测试数据库打开失败: {error}"),
    };
    let session = learning_session();
    if let Err(error) = database.learning().start(&session) {
        panic!("学习会话保存失败: {error}");
    }
    for (id, domain, kind) in [
        (
            "observation-main",
            "www.example.com",
            LearningObservationKind::MainFrame,
        ),
        (
            "observation-api",
            "api.example.com",
            LearningObservationKind::Subresource,
        ),
    ] {
        let observation = observation(id, domain, kind);
        if let Err(error) = database.learning().record_observation(&observation, 2_000) {
            panic!("学习观测保存失败: {error}");
        }
    }
    database
}

fn learning_session() -> LearningSession {
    let context = BrowserContextId::new("context-a");
    let site = DomainName::normalize("www.example.com");
    let (Ok(context), Ok(site)) = (context, site) else {
        panic!("学习会话测试输入无效");
    };
    match LearningSession::start(
        session_id(),
        LearningSubject::Site(site),
        Some(context),
        1_000,
        60_000,
    ) {
        Ok(value) => value,
        Err(error) => panic!("学习会话创建失败: {error}"),
    }
}

fn observation(id: &str, domain: &str, kind: LearningObservationKind) -> LearningObservation {
    let observation_id = ObservationId::new(id);
    let context = BrowserContextId::new("context-a");
    let domain = DomainName::normalize(domain);
    let initiator = DomainName::normalize("www.example.com");
    let (Ok(observation_id), Ok(context), Ok(domain), Ok(initiator)) =
        (observation_id, context, domain, initiator)
    else {
        panic!("学习观测测试输入无效");
    };
    LearningObservation::new(
        session_id(),
        observation_id,
        Some(context),
        kind,
        domain,
        Some(initiator),
        LearningResourceType::MainFrame,
        false,
    )
}

fn selection(domain: &str, policy_id: &str) -> LearningPolicySelection {
    selection_with_transports(domain, policy_id, Vec::new())
}

fn selection_with_transports(
    domain: &str,
    policy_id: &str,
    transports: Vec<Transport>,
) -> LearningPolicySelection {
    let domain = DomainName::normalize(domain);
    let id = PolicyId::new(policy_id);
    let (Ok(domain), Ok(id)) = (domain, id) else {
        panic!("确认规则测试输入无效");
    };
    let matcher = DomainMatcher::new(DomainMatchKind::Exact, domain.as_ascii());
    let Ok(matcher) = matcher else {
        panic!("确认规则域名匹配器创建失败: {matcher:?}");
    };
    let policy_match = PolicyMatch::new(None, Some(matcher), None, None, transports, Vec::new());
    let Ok(policy_match) = policy_match else {
        panic!("确认规则匹配器创建失败: {policy_match:?}");
    };
    let policy = Policy::new(
        id,
        format!("直连 {}", domain.as_ascii()),
        policy_match,
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, 1),
    );
    let Ok(policy) = policy else {
        panic!("确认规则创建失败: {policy:?}");
    };
    LearningPolicySelection::new(domain, policy, false)
}

fn session_id() -> LearningSessionId {
    match LearningSessionId::new("learning-a") {
        Ok(value) => value,
        Err(error) => panic!("测试会话 ID 无效: {error}"),
    }
}

fn confirmation_id(value: &str) -> ConfirmationId {
    match ConfirmationId::new(value) {
        Ok(value) => value,
        Err(error) => panic!("测试确认 ID 无效: {error}"),
    }
}
