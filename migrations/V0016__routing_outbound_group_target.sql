-- 默认路由需要与单出口一样显式区分出口组，不能靠字符串或查询目录猜测目标类型。
ALTER TABLE routing_settings RENAME TO routing_settings_legacy;

CREATE TABLE routing_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    default_action TEXT NOT NULL CHECK (default_action IN ('direct', 'proxy')),
    default_outbound_id TEXT REFERENCES outbound(id) ON DELETE RESTRICT,
    default_outbound_group_id TEXT REFERENCES outbound_group(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_unix_ms INTEGER NOT NULL CHECK (updated_at_unix_ms >= 0),
    CHECK (
        (
            default_action = 'direct'
            AND default_outbound_id IS NULL
            AND default_outbound_group_id IS NULL
        )
        OR
        (
            default_action = 'proxy'
            AND (default_outbound_id IS NOT NULL)
                + (default_outbound_group_id IS NOT NULL) = 1
        )
    )
) STRICT;

INSERT INTO routing_settings(
    singleton_id,
    default_action,
    default_outbound_id,
    default_outbound_group_id,
    revision,
    updated_at_unix_ms
)
SELECT
    singleton_id,
    default_action,
    default_outbound_id,
    NULL,
    revision,
    updated_at_unix_ms
FROM routing_settings_legacy;

DROP TABLE routing_settings_legacy;
