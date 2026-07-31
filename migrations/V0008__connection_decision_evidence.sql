-- 扩展连接决策表，使配置、决策、路径和出口证据可以被明确区分。
ALTER TABLE connection_decision
ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE connection_decision
ADD COLUMN provider_generation INTEGER NOT NULL DEFAULT 0
CHECK (provider_generation >= 0);

ALTER TABLE connection_decision
ADD COLUMN flow_id TEXT NOT NULL DEFAULT '';

ALTER TABLE connection_decision
ADD COLUMN app_display_name TEXT;

ALTER TABLE connection_decision
ADD COLUMN app_platform INTEGER NOT NULL DEFAULT 1
CHECK (app_platform BETWEEN 1 AND 2);

ALTER TABLE connection_decision
ADD COLUMN matched_rule_id TEXT;

ALTER TABLE connection_decision
ADD COLUMN failure_mode INTEGER NOT NULL DEFAULT 1
CHECK (failure_mode BETWEEN 1 AND 2);

ALTER TABLE connection_decision
ADD COLUMN evidence_level INTEGER NOT NULL DEFAULT 2
CHECK (evidence_level BETWEEN 2 AND 4);

ALTER TABLE connection_decision
ADD COLUMN interface_name TEXT;

ALTER TABLE connection_decision
ADD COLUMN outbound_id TEXT;

ALTER TABLE connection_decision
ADD COLUMN exit_probe_id TEXT;

ALTER TABLE connection_decision
ADD COLUMN decision_latency_us INTEGER
CHECK (decision_latency_us IS NULL OR decision_latency_us >= 0);

ALTER TABLE connection_decision
ADD COLUMN error_code TEXT;

CREATE UNIQUE INDEX connection_decision_provider_flow
ON connection_decision(provider_id, provider_generation, flow_id)
WHERE flow_id <> '';

CREATE INDEX connection_decision_recent
ON connection_decision(occurred_at_unix_ms DESC, event_id DESC);

-- 证据等级必须与实际可定位的路径信息一致，防止绕过 Repository 写出虚假证据。
CREATE TRIGGER connection_decision_evidence_insert
BEFORE INSERT ON connection_decision
WHEN
    (NEW.provider_id <> 'legacy'
        AND (NEW.provider_generation <= 0 OR NEW.flow_id = ''))
    OR (NEW.error_code IS NOT NULL AND NEW.evidence_level <> 2)
    OR CASE NEW.evidence_level
        WHEN 2 THEN NOT (
            NEW.interface_name IS NULL
            AND NEW.outbound_id IS NULL
            AND NEW.exit_probe_id IS NULL
        )
        WHEN 3 THEN NOT (
            NEW.exit_probe_id IS NULL
            AND (
                (NEW.decision_action = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
            )
        )
        WHEN 4 THEN NOT (
            NEW.exit_probe_id IS NOT NULL
            AND NEW.exit_probe_id <> ''
            AND (
                (NEW.decision_action = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
            )
        )
        ELSE 1
    END
BEGIN
    SELECT RAISE(ABORT, 'connection_decision evidence is inconsistent');
END;

CREATE TRIGGER connection_decision_evidence_update
BEFORE UPDATE ON connection_decision
WHEN
    (NEW.provider_id <> 'legacy'
        AND (NEW.provider_generation <= 0 OR NEW.flow_id = ''))
    OR (NEW.error_code IS NOT NULL AND NEW.evidence_level <> 2)
    OR CASE NEW.evidence_level
        WHEN 2 THEN NOT (
            NEW.interface_name IS NULL
            AND NEW.outbound_id IS NULL
            AND NEW.exit_probe_id IS NULL
        )
        WHEN 3 THEN NOT (
            NEW.exit_probe_id IS NULL
            AND (
                (NEW.decision_action = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
            )
        )
        WHEN 4 THEN NOT (
            NEW.exit_probe_id IS NOT NULL
            AND NEW.exit_probe_id <> ''
            AND (
                (NEW.decision_action = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
            )
        )
        ELSE 1
    END
BEGIN
    SELECT RAISE(ABORT, 'connection_decision evidence is inconsistent');
END;
