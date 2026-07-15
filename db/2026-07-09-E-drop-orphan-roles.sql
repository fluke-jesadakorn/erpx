-- 2026-07-09-E — Remove orphan RBAC scaffolding persona roles.
--
-- L1, L2A, L2B, L3, L4, HQ, DEPT were seeded during an earlier RBAC migration
-- (db/rbac/seed.sql, db/rbac/0017_view_slip_detail.sql) but never assigned to
-- any active user. They have role_permissions rows (unused) but no user_roles
-- entries and no tiles reference them. Safe to remove.
-- role_permissions FK has ON DELETE CASCADE so the dependent rows go with them.

DELETE FROM perm.roles
 WHERE kind = 'persona'
   AND id IN ('L1','L2A','L2B','L3','L4','HQ','DEPT');