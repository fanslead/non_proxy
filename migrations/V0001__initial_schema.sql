-- NonProxy 初始权威存储结构。已发布 migration 不得原地修改。
CREATE TABLE schema_migration (
    version INTEGER PRIMARY KEY CHECK (version > 0),
    name TEXT NOT NULL UNIQUE,
    checksum BLOB NOT NULL CHECK (length(checksum) = 32),
    applied_at_unix_ms INTEGER NOT NULL CHECK (applied_at_unix_ms >= 0)
) STRICT;

CREATE TABLE outbound (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    endpoint_host TEXT,
    endpoint_port INTEGER CHECK (endpoint_port BETWEEN 1 AND 65535),
    credential_reference TEXT,
    credential_kind TEXT,
    credential_label TEXT,
    credential_version INTEGER CHECK (credential_version IS NULL OR credential_version > 0),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK (
        (credential_reference IS NULL AND credential_kind IS NULL AND credential_label IS NULL AND credential_version IS NULL)
        OR
        (credential_reference IS NOT NULL AND credential_kind IS NOT NULL AND credential_label IS NOT NULL AND credential_version IS NOT NULL)
    )
) STRICT;

CREATE TABLE outbound_group (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    strategy TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0)
) STRICT;

CREATE TABLE outbound_group_member (
    group_id TEXT NOT NULL REFERENCES outbound_group(id) ON DELETE CASCADE,
    outbound_id TEXT NOT NULL REFERENCES outbound(id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (group_id, outbound_id),
    UNIQUE (group_id, position)
) STRICT;

CREATE TABLE network_profile (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    fingerprint_kind TEXT NOT NULL,
    fingerprint_value TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0)
) STRICT;

CREATE TABLE policy (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    source_kind INTEGER NOT NULL CHECK (source_kind BETWEEN 1 AND 8),
    decision_action INTEGER NOT NULL CHECK (decision_action BETWEEN 1 AND 3),
    outbound_id TEXT REFERENCES outbound(id) ON DELETE RESTRICT,
    failure_mode INTEGER NOT NULL CHECK (failure_mode BETWEEN 1 AND 2),
    priority INTEGER NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    origin INTEGER NOT NULL CHECK (origin BETWEEN 1 AND 5),
    revision INTEGER NOT NULL CHECK (revision > 0),
    app_platform INTEGER CHECK (app_platform BETWEEN 1 AND 2),
    app_stable_id TEXT,
    app_signer_id TEXT,
    app_include_helpers INTEGER CHECK (app_include_helpers IN (0, 1)),
    cidr TEXT,
    network_profile_id TEXT REFERENCES network_profile(id) ON DELETE RESTRICT,
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK (
        (app_platform IS NULL AND app_stable_id IS NULL AND app_signer_id IS NULL AND app_include_helpers IS NULL)
        OR
        (app_platform IS NOT NULL AND app_stable_id IS NOT NULL AND app_include_helpers IS NOT NULL)
    ),
    CHECK (
        (decision_action = 2 AND outbound_id IS NOT NULL)
        OR
        (decision_action IN (1, 3) AND outbound_id IS NULL)
    )
) STRICT;

CREATE TABLE domain_target (
    policy_id TEXT PRIMARY KEY REFERENCES policy(id) ON DELETE CASCADE,
    match_kind INTEGER NOT NULL CHECK (match_kind BETWEEN 1 AND 3),
    ascii_pattern TEXT NOT NULL
) STRICT;

CREATE TABLE policy_transport (
    policy_id TEXT NOT NULL REFERENCES policy(id) ON DELETE CASCADE,
    transport INTEGER NOT NULL CHECK (transport BETWEEN 1 AND 2),
    PRIMARY KEY (policy_id, transport)
) STRICT;

CREATE TABLE policy_port_range (
    policy_id TEXT NOT NULL REFERENCES policy(id) ON DELETE CASCADE,
    first_port INTEGER NOT NULL CHECK (first_port BETWEEN 1 AND 65535),
    last_port INTEGER NOT NULL CHECK (last_port BETWEEN first_port AND 65535),
    PRIMARY KEY (policy_id, first_port, last_port)
) STRICT;

CREATE TABLE app_identity (
    id INTEGER PRIMARY KEY,
    platform INTEGER NOT NULL CHECK (platform BETWEEN 1 AND 2),
    stable_id TEXT NOT NULL,
    signer_id TEXT,
    executable_hash BLOB,
    executable_path_hint TEXT,
    display_name TEXT,
    parent_stable_id TEXT,
    helper_group_id TEXT,
    first_seen_at_unix_ms INTEGER NOT NULL CHECK (first_seen_at_unix_ms >= 0),
    last_seen_at_unix_ms INTEGER NOT NULL CHECK (last_seen_at_unix_ms >= first_seen_at_unix_ms)
) STRICT;

CREATE UNIQUE INDEX app_identity_unique_signer
ON app_identity(platform, stable_id, COALESCE(signer_id, ''));

CREATE TABLE app_identity_alias (
    platform INTEGER NOT NULL CHECK (platform BETWEEN 1 AND 2),
    alias_stable_id TEXT NOT NULL,
    canonical_identity_id INTEGER NOT NULL REFERENCES app_identity(id) ON DELETE CASCADE,
    evidence_kind TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    PRIMARY KEY (platform, alias_stable_id)
) STRICT;

CREATE TABLE policy_snapshot (
    snapshot_version INTEGER PRIMARY KEY CHECK (snapshot_version > 0),
    source_snapshot_version INTEGER REFERENCES policy_snapshot(snapshot_version) ON DELETE RESTRICT,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
    policy_count INTEGER NOT NULL CHECK (policy_count >= 0),
    payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 16777216),
    status TEXT NOT NULL CHECK (status IN ('pending', 'active', 'superseded', 'rejected')),
    activated_at_unix_ms INTEGER,
    failure_code TEXT,
    CHECK (
        source_snapshot_version IS NULL
        OR source_snapshot_version < snapshot_version
    ),
    CHECK (
        (status IN ('pending', 'rejected') AND activated_at_unix_ms IS NULL)
        OR
        (status IN ('active', 'superseded') AND activated_at_unix_ms IS NOT NULL)
    ),
    CHECK (
        (status = 'rejected' AND failure_code IS NOT NULL)
        OR
        (status != 'rejected' AND failure_code IS NULL)
    )
) STRICT;

CREATE UNIQUE INDEX one_pending_policy_snapshot
ON policy_snapshot(status)
WHERE status = 'pending';

CREATE UNIQUE INDEX one_active_policy_snapshot
ON policy_snapshot(status)
WHERE status = 'active';

CREATE TABLE policy_snapshot_ack (
    snapshot_version INTEGER NOT NULL REFERENCES policy_snapshot(snapshot_version) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    provider_generation INTEGER NOT NULL CHECK (provider_generation >= 0),
    content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('loaded', 'rejected')),
    error_code TEXT,
    acknowledged_at_unix_ms INTEGER NOT NULL CHECK (acknowledged_at_unix_ms >= 0),
    PRIMARY KEY (snapshot_version, provider_id),
    CHECK (
        (state = 'loaded' AND error_code IS NULL)
        OR
        (state = 'rejected' AND error_code IS NOT NULL)
    )
) STRICT;

CREATE TABLE audit_event (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    occurred_at_unix_ms INTEGER NOT NULL CHECK (occurred_at_unix_ms >= 0),
    snapshot_version INTEGER,
    policy_id TEXT,
    reason_code TEXT,
    details TEXT
) STRICT;

CREATE TABLE connection_decision (
    event_id TEXT PRIMARY KEY,
    occurred_at_unix_ms INTEGER NOT NULL CHECK (occurred_at_unix_ms >= 0),
    snapshot_version INTEGER NOT NULL,
    app_stable_id TEXT NOT NULL,
    destination_redacted TEXT NOT NULL,
    transport INTEGER NOT NULL CHECK (transport BETWEEN 1 AND 2),
    destination_port INTEGER NOT NULL CHECK (destination_port BETWEEN 1 AND 65535),
    matched_policy_id TEXT,
    decision_action INTEGER NOT NULL CHECK (decision_action BETWEEN 1 AND 3),
    reason_code TEXT NOT NULL
) STRICT;

CREATE INDEX connection_decision_retention
ON connection_decision(occurred_at_unix_ms);

CREATE TABLE dns_observation (
    event_id TEXT PRIMARY KEY,
    occurred_at_unix_ms INTEGER NOT NULL CHECK (occurred_at_unix_ms >= 0),
    app_stable_id TEXT NOT NULL,
    query_name_redacted TEXT NOT NULL,
    answer_family INTEGER CHECK (answer_family IN (4, 6)),
    ttl_seconds INTEGER CHECK (ttl_seconds >= 0)
) STRICT;

CREATE INDEX dns_observation_retention
ON dns_observation(occurred_at_unix_ms);

CREATE TABLE learning_session (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    state TEXT NOT NULL,
    started_at_unix_ms INTEGER NOT NULL CHECK (started_at_unix_ms >= 0),
    stopped_at_unix_ms INTEGER
) STRICT;

CREATE TABLE learning_candidate (
    session_id TEXT NOT NULL REFERENCES learning_session(id) ON DELETE CASCADE,
    candidate_key TEXT NOT NULL,
    classification TEXT NOT NULL,
    confidence_millis INTEGER NOT NULL CHECK (confidence_millis BETWEEN 0 AND 1000),
    evidence_count INTEGER NOT NULL CHECK (evidence_count >= 0),
    last_seen_at_unix_ms INTEGER NOT NULL CHECK (last_seen_at_unix_ms >= 0),
    PRIMARY KEY (session_id, candidate_key)
) STRICT;

CREATE TABLE adapter_state (
    adapter_id TEXT PRIMARY KEY,
    adapter_version TEXT NOT NULL,
    capability_hash BLOB NOT NULL CHECK (length(capability_hash) = 32),
    state TEXT NOT NULL,
    last_sync_at_unix_ms INTEGER,
    last_error_code TEXT
) STRICT;

CREATE TABLE health_probe (
    probe_id TEXT PRIMARY KEY,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    checked_at_unix_ms INTEGER NOT NULL CHECK (checked_at_unix_ms >= 0),
    status TEXT NOT NULL,
    latency_ms INTEGER CHECK (latency_ms >= 0),
    error_code TEXT
) STRICT;
