-- 保留经过 Provider 认证的应用签名与辅助进程归属，供活动证据和安全快捷规则使用。
ALTER TABLE connection_decision
ADD COLUMN app_signer_id TEXT
CHECK (
    app_signer_id IS NULL
    OR (
        length(app_signer_id) BETWEEN 1 AND 512
        AND app_signer_id = trim(app_signer_id)
    )
);

ALTER TABLE connection_decision
ADD COLUMN app_parent_stable_id TEXT
CHECK (
    app_parent_stable_id IS NULL
    OR (
        length(app_parent_stable_id) BETWEEN 1 AND 512
        AND app_parent_stable_id = trim(app_parent_stable_id)
    )
);

ALTER TABLE connection_decision
ADD COLUMN app_helper_group_id TEXT
CHECK (
    app_helper_group_id IS NULL
    OR (
        length(app_helper_group_id) BETWEEN 1 AND 512
        AND app_helper_group_id = trim(app_helper_group_id)
    )
);
