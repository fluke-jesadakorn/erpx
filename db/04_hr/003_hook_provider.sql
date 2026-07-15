-- folio/db/04_hr/003_hook_provider.sql
-- Register the HR LINE OA in hook.hook_providers.
-- Wave 2-A: separate channel so webhooks verify against HR_LINE_CHANNEL_SECRET.
-- Idempotent.

INSERT INTO hook.hook_providers (id, display_name, kind, secret_env)
VALUES ('line_hr', 'HR LINE OA', 'line', 'HR_LINE_CHANNEL_SECRET')
ON CONFLICT (id) DO NOTHING;
