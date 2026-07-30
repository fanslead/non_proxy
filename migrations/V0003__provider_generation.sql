-- Provider 注册代数必须跨 gatewayd 重启保持单调，避免旧确认覆盖新会话。
CREATE TABLE provider_generation (
    provider_id TEXT PRIMARY KEY,
    generation INTEGER NOT NULL CHECK (
        generation > 0
        AND generation < 9223372036854775807
    )
) STRICT;
