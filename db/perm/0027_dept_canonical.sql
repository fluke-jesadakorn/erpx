-- Folio — Canonical department display names.
--
-- The three departments below drifted between the DB seed and the UI
-- maps in PersonaMenu.tsx + OrgChartV2.tsx + seed_ai_settings.js.
-- The UI wins (3 occurrences) over the DB seed (1 occurrence).

BEGIN;

UPDATE perm.roles SET display_name = 'Finance & Account' WHERE id = 'dept-finance-2';
UPDATE perm.roles SET display_name = 'IT'                 WHERE id = 'dept-it';
UPDATE perm.roles SET display_name = 'HR'                 WHERE id = 'dept-hr-2';

COMMIT;