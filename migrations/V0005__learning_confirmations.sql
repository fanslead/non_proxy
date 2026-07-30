-- 学习候选确认：保存幂等确认收据和每个候选的用户决策。
CREATE TABLE learning_confirmation (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
    session_id TEXT NOT NULL UNIQUE
        REFERENCES learning_session(id) ON DELETE CASCADE,
    confirmed_at_unix_ms INTEGER NOT NULL CHECK (confirmed_at_unix_ms >= 0),
    selected_count INTEGER NOT NULL CHECK (selected_count BETWEEN 1 AND 256),
    snapshot_version INTEGER CHECK (snapshot_version > 0)
) STRICT;

CREATE TABLE learning_candidate_decision (
    confirmation_id TEXT NOT NULL
        REFERENCES learning_confirmation(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    candidate_key TEXT NOT NULL,
    selected INTEGER NOT NULL CHECK (selected IN (0, 1)),
    policy_id TEXT,
    CHECK (
        (selected = 1 AND policy_id IS NOT NULL)
        OR
        (selected = 0 AND policy_id IS NULL)
    ),
    PRIMARY KEY (confirmation_id, candidate_key),
    FOREIGN KEY (session_id, candidate_key)
        REFERENCES learning_candidate(session_id, candidate_key)
        ON DELETE CASCADE
) STRICT;

CREATE INDEX learning_candidate_decision_session
ON learning_candidate_decision(session_id, selected, candidate_key);
