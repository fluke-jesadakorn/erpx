BEGIN;

INSERT INTO folio.ai_assignments
  (section_key, task_type, provider_id, model_id, params_json, priority, enabled, user_selectable)
SELECT 'staff:ocr', 'vision', p.id, m.id, '{}'::jsonb, 50, true, true
FROM folio.ai_providers p
JOIN folio.ai_models m ON m.provider_id = p.id
WHERE p.name = 'local-ollama'
  AND m.name = 'qwen3-vl:4b'
ON CONFLICT (section_key, task_type, priority) DO UPDATE
SET provider_id = EXCLUDED.provider_id,
    model_id = EXCLUDED.model_id,
    enabled = true,
    user_selectable = true,
    updated_at = now();

COMMIT;
