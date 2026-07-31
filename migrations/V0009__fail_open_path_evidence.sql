-- 显式保存 PROXY 策略失败后由 fail-open 建立的物理直连路径。
ALTER TABLE connection_decision
ADD COLUMN fail_open_direct INTEGER NOT NULL DEFAULT 0
CHECK (fail_open_direct BETWEEN 0 AND 1);

DROP TRIGGER connection_decision_evidence_insert;
DROP TRIGGER connection_decision_evidence_update;

-- 同一套约束同时用于 INSERT/UPDATE，防止 Repository 之外的写入夸大证据。
CREATE TRIGGER connection_decision_evidence_insert
BEFORE INSERT ON connection_decision
WHEN
    (NEW.provider_id <> 'legacy'
        AND (NEW.provider_generation <= 0 OR NEW.flow_id = ''))
    OR (NEW.error_code IS NOT NULL
        AND NEW.evidence_level <> 2
        AND NEW.fail_open_direct <> 1)
    OR (NEW.fail_open_direct = 1 AND NEW.error_code IS NULL)
    OR CASE NEW.evidence_level
        WHEN 2 THEN NOT (
            NEW.fail_open_direct = 0
            AND NEW.interface_name IS NULL
            AND NEW.outbound_id IS NULL
            AND NEW.exit_probe_id IS NULL
        )
        WHEN 3 THEN NOT (
            NEW.exit_probe_id IS NULL
            AND (
                (NEW.decision_action = 1
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
                OR
                (NEW.decision_action = 2
                    AND NEW.failure_mode = 2
                    AND NEW.fail_open_direct = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
            )
        )
        WHEN 4 THEN NOT (
            NEW.exit_probe_id IS NOT NULL
            AND NEW.exit_probe_id <> ''
            AND (
                (NEW.decision_action = 1
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
                OR
                (NEW.decision_action = 2
                    AND NEW.failure_mode = 2
                    AND NEW.fail_open_direct = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
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
    OR (NEW.error_code IS NOT NULL
        AND NEW.evidence_level <> 2
        AND NEW.fail_open_direct <> 1)
    OR (NEW.fail_open_direct = 1 AND NEW.error_code IS NULL)
    OR CASE NEW.evidence_level
        WHEN 2 THEN NOT (
            NEW.fail_open_direct = 0
            AND NEW.interface_name IS NULL
            AND NEW.outbound_id IS NULL
            AND NEW.exit_probe_id IS NULL
        )
        WHEN 3 THEN NOT (
            NEW.exit_probe_id IS NULL
            AND (
                (NEW.decision_action = 1
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
                OR
                (NEW.decision_action = 2
                    AND NEW.failure_mode = 2
                    AND NEW.fail_open_direct = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
            )
        )
        WHEN 4 THEN NOT (
            NEW.exit_probe_id IS NOT NULL
            AND NEW.exit_probe_id <> ''
            AND (
                (NEW.decision_action = 1
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
                OR
                (NEW.decision_action = 2
                    AND NEW.fail_open_direct = 0
                    AND NEW.interface_name IS NULL
                    AND NEW.outbound_id IS NOT NULL
                    AND NEW.outbound_id <> '')
                OR
                (NEW.decision_action = 2
                    AND NEW.failure_mode = 2
                    AND NEW.fail_open_direct = 1
                    AND NEW.interface_name IS NOT NULL
                    AND NEW.interface_name <> ''
                    AND NEW.outbound_id IS NULL)
            )
        )
        ELSE 1
    END
BEGIN
    SELECT RAISE(ABORT, 'connection_decision evidence is inconsistent');
END;
