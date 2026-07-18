-- 2026-07-25-C-ai-law-sections.sql
-- Add ai_assignments rows for the law:* AI section keys (law:rag chat,
-- law:contracts embed) so the law module can resolve providers.
-- Idempotent.

BEGIN;

INSERT INTO folio.ai_assignments (section_key, task_type, model_id, enabled, priority, params_json)
SELECT 'law:rag', 'chat', m.id, true, 100, '{}'::jsonb
  FROM folio.ai_models m
 WHERE m.enabled
   AND 'chat' = ANY(m.capabilities)
 ORDER BY m.id
 LIMIT 1
ON CONFLICT (section_key, task_type, priority) DO NOTHING;

INSERT INTO folio.ai_assignments (section_key, task_type, model_id, enabled, priority, params_json)
SELECT 'law:contracts', 'embed', m.id, true, 100, '{}'::jsonb
  FROM folio.ai_models m
 WHERE m.enabled
   AND 'embed' = ANY(m.capabilities)
 ORDER BY m.id
 LIMIT 1
ON CONFLICT (section_key, task_type, priority) DO NOTHING;

COMMIT;
