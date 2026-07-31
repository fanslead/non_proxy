-- 保存 gatewayd 独立验签后的路由级公网出口回执，不与任意用户连接强行绑定。
CREATE TABLE exit_probe_receipt (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    probe_id TEXT NOT NULL UNIQUE
        CHECK (
            length(probe_id) = 43
            AND probe_id NOT GLOB '*[^A-Za-z0-9_-]*'
        ),
    route_kind INTEGER NOT NULL
        CHECK (route_kind IN (1, 2)),
    outbound_id TEXT
        CHECK (
            outbound_id IS NULL
            OR (
                length(outbound_id) BETWEEN 1 AND 128
                AND outbound_id NOT GLOB '*[ /\\]*'
            )
        ),
    observed_ip TEXT NOT NULL
        CHECK (
            length(observed_ip) BETWEEN 3 AND 45
            AND observed_ip NOT GLOB '*[^0-9A-Fa-f:.]*'
        ),
    ip_family INTEGER NOT NULL
        CHECK (ip_family IN (1, 2)),
    observed_at_unix_ms INTEGER NOT NULL
        CHECK (observed_at_unix_ms > 0),
    key_id TEXT NOT NULL
        CHECK (
            length(key_id) = 22
            AND key_id NOT GLOB '*[^A-Za-z0-9_-]*'
        ),
    verified_at_unix_ms INTEGER NOT NULL
        CHECK (verified_at_unix_ms > 0),
    CHECK (
        (route_kind = 1 AND outbound_id IS NULL)
        OR
        (route_kind = 2 AND outbound_id IS NOT NULL)
    ),
    CHECK (
        observed_at_unix_ms >= verified_at_unix_ms - 120000
        AND observed_at_unix_ms <= verified_at_unix_ms + 300000
    )
);

CREATE INDEX exit_probe_receipt_recent
ON exit_probe_receipt(sequence DESC);

CREATE INDEX exit_probe_receipt_route_recent
ON exit_probe_receipt(route_kind, outbound_id, sequence DESC);

-- 回执是已经验签的审计事实，只允许由有界保留任务删除，不能原地改写。
CREATE TRIGGER exit_probe_receipt_immutable
BEFORE UPDATE ON exit_probe_receipt
BEGIN
    SELECT RAISE(ABORT, 'exit probe receipt is immutable');
END;
