-- Folio — perm.roles gets a numeric level (1 = highest authority, 10 = lowest).
-- Replaces rbac.roles.level / rbac.roles.default_staff_level.
-- Level is set by HR when creating/editing a role.
-- A user's effective_level = MIN(level) of their assigned roles.

BEGIN;

ALTER TABLE perm.roles
  ADD COLUMN IF NOT EXISTS level smallint NOT NULL DEFAULT 5
  CHECK (level BETWEEN 1 AND 10);

CREATE INDEX IF NOT EXISTS perm_roles_level_idx ON perm.roles (level);

COMMENT ON COLUMN perm.roles.level IS
  '1 = C-level/CEO, 10 = lowest. HR sets per role. '
  'User effective_level = MIN(level) of their assigned roles.';

-- Backfill: real personas get persona-appropriate levels.
-- Tier anchors (HQ/L*/DEPT) get levels matching their tier.
UPDATE perm.roles SET level = 1 WHERE id IN ('ceo');
UPDATE perm.roles SET level = 2 WHERE id IN ('cfo', 'admin', 'it', 'HQ');
UPDATE perm.roles SET level = 3 WHERE id IN ('hr_manager', 'finance', 'manager', 'accounting_manager', 'DEPT', 'L3');
UPDATE perm.roles SET level = 4 WHERE id IN ('account_supervisor', 'supervisor', 'account_officer', 'L2B');
UPDATE perm.roles SET level = 5 WHERE id IN ('accountant', 'hr', 'staff', 'L2A');
UPDATE perm.roles SET level = 6 WHERE id IN ('L1');

COMMIT;