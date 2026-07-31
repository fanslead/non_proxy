CREATE UNIQUE INDEX network_profile_unique_fingerprint
ON network_profile(fingerprint_kind, fingerprint_value);

INSERT INTO control_generation(name, value)
VALUES ('network_profile_catalog', 0);
