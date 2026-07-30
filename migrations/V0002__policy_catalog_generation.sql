-- 策略目录代数用于跨页读取时检测并发变化。
CREATE TABLE control_generation (
    name TEXT PRIMARY KEY,
    value INTEGER NOT NULL CHECK (value >= 0)
) STRICT;

INSERT INTO control_generation(name, value)
VALUES ('policy_catalog', 0);
