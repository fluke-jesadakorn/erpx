-- IT Staff role for managing AI Settings / platform configuration
-- ON CONFLICT so re-running is safe

-- Reset sequence in case it drifted from row ids
SELECT setval('roles_id_seq', GREATEST((SELECT MAX(id) FROM roles), 1));

INSERT INTO roles (name) VALUES ('it') ON CONFLICT (name) DO NOTHING;