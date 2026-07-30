use nonproxy_learning::{
    AppLearningSubject, BrowserContextId, LearningCandidateKind, LearningObservation,
    LearningObservationKind, LearningResourceType, LearningSession, LearningSessionId,
    LearningSessionState, LearningSubject, ObservationId,
};
use nonproxy_model::{AppIdentity, DomainName, Platform};
use nonproxy_storage::{PolicyDatabase, StorageError};

#[test]
fn site_learning_is_tab_scoped_idempotent_and_persistent() {
    let mut database = open_database();
    let session = site_session("learning-a", "tab-a", 1_000);
    if let Err(error) = database.learning().start(&session) {
        panic!("学习会话保存失败: {error}");
    }

    let observation = observation("learning-a", "observation-a", "tab-a", "api.example.com");
    let first = database.learning().record_observation(&observation, 2_000);
    let Ok(first) = first else {
        panic!("首次学习观测失败: {first:?}");
    };
    let replay = database.learning().record_observation(&observation, 2_100);
    let Ok(replay) = replay else {
        panic!("幂等学习观测失败: {replay:?}");
    };

    assert!(!first.duplicate());
    assert!(replay.duplicate());
    assert_eq!(replay.candidate().evidence_count(), 1);
    assert_eq!(
        replay.candidate().kind(),
        LearningCandidateKind::RequiredFirstParty
    );

    let listed = database.learning().list_candidates(session.id(), 2_200);
    let Ok((stored_session, candidates)) = listed else {
        panic!("学习候选读取失败: {listed:?}");
    };
    assert_eq!(stored_session, session);
    assert_eq!(candidates, vec![first.candidate().clone()]);
}

#[test]
fn cross_tab_observation_is_rejected_without_leaking_a_candidate() {
    let mut database = open_database();
    let session = site_session("learning-a", "tab-a", 1_000);
    if let Err(error) = database.learning().start(&session) {
        panic!("学习会话保存失败: {error}");
    }
    let wrong_tab = observation("learning-a", "observation-a", "tab-b", "api.example.com");

    let result = database.learning().record_observation(&wrong_tab, 2_000);

    assert!(matches!(result, Err(StorageError::Learning(_))));
    let listed = database.learning().list_candidates(session.id(), 2_100);
    assert!(matches!(listed, Ok((_, candidates)) if candidates.is_empty()));
}

#[test]
fn expiration_and_stop_are_authoritative_and_idempotent() {
    let mut database = open_database();
    let session = site_session("learning-a", "tab-a", 1_000);
    if let Err(error) = database.learning().start(&session) {
        panic!("学习会话保存失败: {error}");
    }

    let first_stop = database.learning().stop(session.id(), 2_000);
    let Ok(first_stop) = first_stop else {
        panic!("停止学习会话失败: {first_stop:?}");
    };
    let replay = database.learning().stop(session.id(), 2_100);
    let Ok(replay) = replay else {
        panic!("重复停止学习会话失败: {replay:?}");
    };
    assert_eq!(first_stop.session().state(), LearningSessionState::Stopped);
    assert_eq!(replay.session(), first_stop.session());

    let expiring = site_session("learning-b", "tab-b", 3_000);
    if let Err(error) = database.learning().start(&expiring) {
        panic!("过期测试会话保存失败: {error}");
    }
    let expired = database.learning().get(expiring.id(), 63_000);
    assert!(matches!(
        expired,
        Ok(Some(value)) if value.state() == LearningSessionState::Expired
            && value.stopped_at_unix_ms() == Some(63_000)
    ));
}

#[test]
fn the_same_tab_and_target_cannot_start_twice_until_stopped() {
    let mut database = open_database();
    let first = site_session("learning-a", "tab-a", 1_000);
    let duplicate = site_session("learning-b", "tab-a", 2_000);
    if let Err(error) = database.learning().start(&first) {
        panic!("首个学习会话保存失败: {error}");
    }

    assert!(matches!(
        database.learning().start(&duplicate),
        Err(StorageError::ActiveLearningSessionExists)
    ));
    if let Err(error) = database.learning().stop(first.id(), 3_000) {
        panic!("首个学习会话停止失败: {error}");
    }
    assert!(database.learning().start(&duplicate).is_ok());
}

#[test]
fn app_learning_round_trip_preserves_platform_and_signer_without_browser_context() {
    let mut database = open_database();
    let identity = AppIdentity::new(Platform::Windows, "com.example.desktop")
        .and_then(|value| value.with_signer_id("PUBLISHER-A"));
    let Ok(identity) = identity else {
        panic!("应用学习测试身份无效: {identity:?}");
    };
    let id = LearningSessionId::new("learning-app");
    let Ok(id) = id else {
        panic!("应用学习测试会话 ID 无效: {id:?}");
    };
    let session = LearningSession::start(
        id,
        LearningSubject::App(AppLearningSubject::from_identity(&identity)),
        None,
        1_000,
        60_000,
    );
    let Ok(session) = session else {
        panic!("应用学习测试会话创建失败: {session:?}");
    };
    if let Err(error) = database.learning().start(&session) {
        panic!("应用学习测试会话保存失败: {error}");
    }

    assert!(matches!(
        database.learning().get(session.id(), 2_000),
        Ok(Some(stored)) if stored == session
    ));
}

fn open_database() -> PolicyDatabase {
    match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("学习测试数据库打开失败: {error}"),
    }
}

fn site_session(id: &str, context: &str, started_at: u64) -> LearningSession {
    let id = LearningSessionId::new(id);
    let context = BrowserContextId::new(context);
    let site = DomainName::normalize("www.example.com");
    let (Ok(id), Ok(context), Ok(site)) = (id, context, site) else {
        panic!("学习测试会话输入无效");
    };
    match LearningSession::start(
        id,
        LearningSubject::Site(site),
        Some(context),
        started_at,
        60_000,
    ) {
        Ok(value) => value,
        Err(error) => panic!("学习测试会话创建失败: {error}"),
    }
}

fn observation(
    session_id: &str,
    observation_id: &str,
    context: &str,
    domain: &str,
) -> LearningObservation {
    let session_id = LearningSessionId::new(session_id);
    let observation_id = ObservationId::new(observation_id);
    let context = BrowserContextId::new(context);
    let domain = DomainName::normalize(domain);
    let initiator = DomainName::normalize("www.example.com");
    let (Ok(session_id), Ok(observation_id), Ok(context), Ok(domain), Ok(initiator)) =
        (session_id, observation_id, context, domain, initiator)
    else {
        panic!("学习测试观测输入无效");
    };
    LearningObservation::new(
        session_id,
        observation_id,
        Some(context),
        LearningObservationKind::Subresource,
        domain,
        Some(initiator),
        LearningResourceType::Fetch,
        false,
    )
}
