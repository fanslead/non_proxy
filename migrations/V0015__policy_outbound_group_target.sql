-- SQLite 不能原地放宽已发布的 policy CHECK，使用受控表重建增加显式出口组目标。
ALTER TABLE policy RENAME TO policy_legacy;

CREATE TABLE policy (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    source_kind INTEGER NOT NULL CHECK (source_kind BETWEEN 1 AND 8),
    decision_action INTEGER NOT NULL CHECK (decision_action BETWEEN 1 AND 3),
    outbound_id TEXT REFERENCES outbound(id) ON DELETE RESTRICT,
    outbound_group_id TEXT REFERENCES outbound_group(id) ON DELETE RESTRICT,
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
        (
            decision_action = 2
            AND (outbound_id IS NOT NULL) + (outbound_group_id IS NOT NULL) = 1
        )
        OR
        (
            decision_action IN (1, 3)
            AND outbound_id IS NULL
            AND outbound_group_id IS NULL
        )
    )
) STRICT;

INSERT INTO policy(
    id, display_name, source_kind, decision_action, outbound_id, outbound_group_id,
    failure_mode, priority, enabled, origin, revision,
    app_platform, app_stable_id, app_signer_id, app_include_helpers,
    cidr, network_profile_id, updated_at_unix_ms
)
SELECT
    id, display_name, source_kind, decision_action, outbound_id, NULL,
    failure_mode, priority, enabled, origin, revision,
    app_platform, app_stable_id, app_signer_id, app_include_helpers,
    cidr, network_profile_id, updated_at_unix_ms
FROM policy_legacy;

-- policy 的三个子表在 ALTER RENAME 后会指向 legacy 名称，显式重建以恢复外键目标。
CREATE TABLE domain_target_v15 (
    policy_id TEXT PRIMARY KEY REFERENCES policy(id) ON DELETE CASCADE,
    match_kind INTEGER NOT NULL CHECK (match_kind BETWEEN 1 AND 3),
    ascii_pattern TEXT NOT NULL
) STRICT;
INSERT INTO domain_target_v15 SELECT * FROM domain_target;

CREATE TABLE policy_transport_v15 (
    policy_id TEXT NOT NULL REFERENCES policy(id) ON DELETE CASCADE,
    transport INTEGER NOT NULL CHECK (transport BETWEEN 1 AND 2),
    PRIMARY KEY (policy_id, transport)
) STRICT;
INSERT INTO policy_transport_v15 SELECT * FROM policy_transport;

CREATE TABLE policy_port_range_v15 (
    policy_id TEXT NOT NULL REFERENCES policy(id) ON DELETE CASCADE,
    first_port INTEGER NOT NULL CHECK (first_port BETWEEN 1 AND 65535),
    last_port INTEGER NOT NULL CHECK (last_port BETWEEN first_port AND 65535),
    PRIMARY KEY (policy_id, first_port, last_port)
) STRICT;
INSERT INTO policy_port_range_v15 SELECT * FROM policy_port_range;

DROP TABLE domain_target;
DROP TABLE policy_transport;
DROP TABLE policy_port_range;
DROP TABLE policy_legacy;

ALTER TABLE domain_target_v15 RENAME TO domain_target;
ALTER TABLE policy_transport_v15 RENAME TO policy_transport;
ALTER TABLE policy_port_range_v15 RENAME TO policy_port_range;
