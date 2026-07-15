-- 2026-07-09-F — Rename persona roles to match new position vocabulary.
--
-- 1. staff → officer. Children (user_roles, role_permissions) hold FKs to
--    perm.roles(id) with ON DELETE CASCADE / NO ACTION on UPDATE, so the
--    rename goes: insert target, migrate children, delete source.
-- 2. finance: keep id, expand display_name 'Finance' → 'Finance Lead'
--    so the badge is unambiguous against dept 'Finance & Account'.

BEGIN;

INSERT INTO perm.roles (id, display_name, kind, level, is_system, sort_order)
  VALUES ('officer', 'Officer', 'persona', 5, true, 13)
  ON CONFLICT (id) DO NOTHING;

UPDATE perm.role_permissions SET role_id = 'officer' WHERE role_id = 'staff';
UPDATE perm.user_roles        SET role_id = 'officer' WHERE role_id = 'staff';
DELETE FROM perm.roles        WHERE id = 'staff';

UPDATE perm.roles             SET display_name = 'Finance Lead' WHERE id = 'finance';

COMMIT;