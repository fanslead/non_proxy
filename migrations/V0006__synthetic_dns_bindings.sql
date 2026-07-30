-- Windows 选择性合成 DNS：保存安装级 IPv6 ULA 前缀与可恢复的域名地址绑定。
CREATE TABLE synthetic_dns_config (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    ipv6_prefix TEXT NOT NULL UNIQUE,
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0)
) STRICT;

CREATE TABLE synthetic_dns_binding (
    family INTEGER NOT NULL CHECK (family IN (4, 6)),
    address TEXT NOT NULL,
    domain_ascii TEXT NOT NULL,
    created_at_unix_ms INTEGER NOT NULL CHECK (created_at_unix_ms >= 0),
    last_issued_at_unix_ms INTEGER NOT NULL
        CHECK (last_issued_at_unix_ms >= created_at_unix_ms),
    retain_until_unix_ms INTEGER NOT NULL
        CHECK (retain_until_unix_ms > last_issued_at_unix_ms),
    PRIMARY KEY (family, address),
    UNIQUE (family, domain_ascii)
) STRICT;

CREATE INDEX synthetic_dns_binding_retention
ON synthetic_dns_binding(retain_until_unix_ms);
