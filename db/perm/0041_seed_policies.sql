-- Seed canonical policy ids so p.ref(...) lookups resolve once DB-driven policies land.
INSERT INTO perm.policies (id, name, ast, description) VALUES
  ('canActOnWaybillStage', 'canActOnWaybillStage', '{"kind":"ref","id":"canActOnWaybillStage"}'::jsonb, 'Stage-aware approval gate'),
  ('deptHasManagerClass', 'deptHasManagerClass', '{"kind":"admin"}'::jsonb, 'Resolver predicate — overridden in registry')
ON CONFLICT (id) DO NOTHING;