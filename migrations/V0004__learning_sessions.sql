-- 学习会话 V2：补齐过期、标签页隔离、候选确认与幂等观测。
-- 已发布 migration 不得原地修改，因此通过新表事务迁移 V1 预留结构。
CREATE TABLE learning_session_v2 (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('app', 'site')),
    target TEXT NOT NULL CHECK (length(target) BETWEEN 1 AND 512),
    app_platform INTEGER CHECK (app_platform BETWEEN 1 AND 2),
    app_signer_id TEXT,
    browser_context_id TEXT,
    state TEXT NOT NULL CHECK (state IN ('active', 'stopped', 'expired')),
    started_at_unix_ms INTEGER NOT NULL CHECK (started_at_unix_ms >= 0),
    expires_at_unix_ms INTEGER NOT NULL CHECK (expires_at_unix_ms > started_at_unix_ms),
    stopped_at_unix_ms INTEGER,
    CHECK (
        (kind = 'app' AND app_platform IS NOT NULL AND browser_context_id IS NULL)
        OR
        (kind = 'site' AND app_platform IS NULL AND app_signer_id IS NULL
            AND browser_context_id IS NOT NULL)
    ),
    CHECK (
        (state = 'active' AND stopped_at_unix_ms IS NULL)
        OR
        (state IN ('stopped', 'expired') AND stopped_at_unix_ms IS NOT NULL)
    ),
    CHECK (stopped_at_unix_ms IS NULL OR stopped_at_unix_ms >= started_at_unix_ms)
) STRICT;

INSERT INTO learning_session_v2(
    id, kind, target, app_platform, app_signer_id, browser_context_id,
    state, started_at_unix_ms, expires_at_unix_ms, stopped_at_unix_ms
)
SELECT
    id,
    CASE kind WHEN 'app' THEN 'app' ELSE 'site' END,
    target,
    CASE kind WHEN 'app' THEN 1 ELSE NULL END,
    NULL,
    CASE kind WHEN 'app' THEN NULL ELSE 'legacy:' || id END,
    CASE WHEN state = 'active' THEN 'active' ELSE 'stopped' END,
    started_at_unix_ms,
    started_at_unix_ms + 60000,
    CASE
        WHEN state = 'active' THEN NULL
        ELSE MAX(COALESCE(stopped_at_unix_ms, started_at_unix_ms), started_at_unix_ms)
    END
FROM learning_session;

CREATE TABLE learning_candidate_v2 (
    session_id TEXT NOT NULL REFERENCES learning_session_v2(id) ON DELETE CASCADE,
    candidate_key TEXT NOT NULL,
    classification TEXT NOT NULL CHECK (
        classification IN (
            'required_first_party', 'likely_api', 'likely_auth',
            'likely_cdn', 'third_party', 'unknown'
        )
    ),
    confidence_millis INTEGER NOT NULL CHECK (confidence_millis BETWEEN 0 AND 1000),
    requires_confirmation INTEGER NOT NULL CHECK (requires_confirmation IN (0, 1)),
    evidence_count INTEGER NOT NULL CHECK (evidence_count > 0),
    first_seen_at_unix_ms INTEGER NOT NULL CHECK (first_seen_at_unix_ms >= 0),
    last_seen_at_unix_ms INTEGER NOT NULL CHECK (last_seen_at_unix_ms >= first_seen_at_unix_ms),
    main_frame_count INTEGER NOT NULL CHECK (main_frame_count >= 0),
    subresource_count INTEGER NOT NULL CHECK (subresource_count >= 0),
    redirect_count INTEGER NOT NULL CHECK (redirect_count >= 0),
    CHECK (main_frame_count + subresource_count + redirect_count = evidence_count),
    PRIMARY KEY (session_id, candidate_key)
) STRICT;

INSERT INTO learning_candidate_v2(
    session_id, candidate_key, classification, confidence_millis,
    requires_confirmation, evidence_count, first_seen_at_unix_ms,
    last_seen_at_unix_ms, main_frame_count, subresource_count, redirect_count
)
SELECT
    session_id,
    candidate_key,
    CASE classification
        WHEN 'required_first_party' THEN 'required_first_party'
        WHEN 'likely_api' THEN 'likely_api'
        WHEN 'likely_auth' THEN 'likely_auth'
        WHEN 'likely_cdn' THEN 'likely_cdn'
        WHEN 'third_party' THEN 'third_party'
        ELSE 'unknown'
    END,
    confidence_millis,
    CASE
        WHEN classification = 'required_first_party' AND confidence_millis >= 900 THEN 0
        ELSE 1
    END,
    MAX(evidence_count, 1),
    last_seen_at_unix_ms,
    last_seen_at_unix_ms,
    0,
    MAX(evidence_count, 1),
    0
FROM learning_candidate;

DROP TABLE learning_candidate;
DROP TABLE learning_session;
ALTER TABLE learning_session_v2 RENAME TO learning_session;
ALTER TABLE learning_candidate_v2 RENAME TO learning_candidate;

CREATE UNIQUE INDEX one_active_learning_subject
ON learning_session(
    kind,
    target,
    COALESCE(app_platform, 0),
    COALESCE(app_signer_id, ''),
    COALESCE(browser_context_id, '')
)
WHERE state = 'active';

CREATE INDEX learning_session_expiration
ON learning_session(state, expires_at_unix_ms);

CREATE INDEX learning_candidate_rank
ON learning_candidate(session_id, requires_confirmation, confidence_millis DESC);

CREATE TABLE learning_observation_receipt (
    session_id TEXT NOT NULL REFERENCES learning_session(id) ON DELETE CASCADE,
    observation_id TEXT NOT NULL,
    candidate_key TEXT NOT NULL,
    observed_at_unix_ms INTEGER NOT NULL CHECK (observed_at_unix_ms >= 0),
    PRIMARY KEY (session_id, observation_id),
    FOREIGN KEY (session_id, candidate_key)
        REFERENCES learning_candidate(session_id, candidate_key) ON DELETE CASCADE
) STRICT;
