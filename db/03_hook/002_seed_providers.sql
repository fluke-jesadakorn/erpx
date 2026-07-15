-- folio/db/03_hook/002_seed_providers.sql
-- Baseline + extended LINE providers. Idempotent.
-- Baseline: 'line' (folio / Law bot) + 'generic' (generic webhook).
-- Extended (added later by feature migration agents):
--   'line_hr' — HR bot via HR_LINE_CHANNEL_SECRET

INSERT INTO hook.hook_providers (id, display_name, kind, secret_env) VALUES
  ('line',    'Folio LINE OA',  'line',    'LINE_CHANNEL_SECRET'),
  ('generic', 'Generic Webhook', 'generic', 'HOOK_GENERIC_SECRET')
ON CONFLICT (id) DO NOTHING;