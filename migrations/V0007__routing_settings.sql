-- 默认路由是权威配置，必须与引用的出口和策略快照保持一致。
CREATE TABLE routing_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    default_action TEXT NOT NULL CHECK (default_action IN ('direct', 'proxy')),
    default_outbound_id TEXT REFERENCES outbound(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK (
        (default_action = 'direct' AND default_outbound_id IS NULL)
        OR
        (default_action = 'proxy' AND default_outbound_id IS NOT NULL)
    )
) STRICT;

INSERT INTO routing_settings(
    singleton_id,
    default_action,
    default_outbound_id,
    revision,
    updated_at_unix_ms
) VALUES (1, 'direct', NULL, 1, 0);
