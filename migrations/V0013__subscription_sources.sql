-- 远程订阅 URL 和 Token 只保存到系统凭据库；SQLite 仅保存引用、刷新状态和节点归属。
CREATE TABLE subscription_source (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    endpoint_credential_reference TEXT NOT NULL UNIQUE,
    endpoint_credential_label TEXT NOT NULL,
    endpoint_credential_version INTEGER NOT NULL CHECK (endpoint_credential_version > 0),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    refresh_interval_seconds INTEGER NOT NULL
        CHECK (refresh_interval_seconds BETWEEN 900 AND 604800),
    revision INTEGER NOT NULL CHECK (revision > 0),
    content_generation INTEGER NOT NULL CHECK (content_generation >= 0),
    consecutive_failures INTEGER NOT NULL CHECK (consecutive_failures >= 0),
    next_refresh_at_unix_ms INTEGER NOT NULL CHECK (next_refresh_at_unix_ms >= 0),
    last_attempted_at_unix_ms INTEGER CHECK (last_attempted_at_unix_ms >= 0),
    last_succeeded_at_unix_ms INTEGER CHECK (last_succeeded_at_unix_ms >= 0),
    last_error_code TEXT,
    content_hash BLOB CHECK (content_hash IS NULL OR length(content_hash) = 32),
    node_count INTEGER NOT NULL CHECK (node_count BETWEEN 0 AND 100),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK (
        last_error_code IS NULL
        OR (
            length(last_error_code) BETWEEN 4 AND 128
            AND substr(last_error_code, 1, 3) = 'NP_'
            AND last_error_code NOT GLOB '*[^A-Z0-9_]*'
            AND last_attempted_at_unix_ms IS NOT NULL
        )
    ),
    CHECK (
        (content_generation = 0 AND last_succeeded_at_unix_ms IS NULL
            AND content_hash IS NULL AND node_count = 0)
        OR
        (content_generation > 0 AND last_succeeded_at_unix_ms IS NOT NULL
            AND content_hash IS NOT NULL AND node_count > 0)
    ),
    CHECK (
        last_succeeded_at_unix_ms IS NULL
        OR (last_attempted_at_unix_ms IS NOT NULL
            AND last_attempted_at_unix_ms >= last_succeeded_at_unix_ms)
    )
) STRICT;

CREATE TABLE subscription_outbound (
    subscription_id TEXT NOT NULL
        REFERENCES subscription_source(id) ON DELETE CASCADE,
    outbound_id TEXT NOT NULL UNIQUE REFERENCES outbound(id) ON DELETE RESTRICT,
    node_key TEXT NOT NULL,
    present INTEGER NOT NULL CHECK (present IN (0, 1)),
    last_seen_generation INTEGER NOT NULL CHECK (last_seen_generation > 0),
    PRIMARY KEY (subscription_id, node_key)
) STRICT;

CREATE INDEX subscription_source_due
ON subscription_source(enabled, next_refresh_at_unix_ms);

CREATE INDEX subscription_outbound_present
ON subscription_outbound(subscription_id, present, outbound_id);
