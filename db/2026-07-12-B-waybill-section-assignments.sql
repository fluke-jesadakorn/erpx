-- 2026-07-12-B: register waybill page AI section assignments.
-- The /waybill/{id} page now wires loadVisionModels() into SettleForm so the
-- payment-slip OCR picker shows MiniMax. We also pin the default provider for
-- the waybill AI sections so any future vision/chat invoke from the page
-- routes to MiniMax by default. Idempotent.

INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, priority, enabled)
SELECT 'waybill:settle', 'vision', mm.id, m.id, 100, true
FROM ai_providers mm
JOIN ai_models m ON m.provider_id = mm.id AND m.name = 'MiniMax-M3'
WHERE mm.name = 'MiniMax'
ON CONFLICT (section_key, task_type, priority) DO UPDATE SET
  provider_id = EXCLUDED.provider_id,
  model_id    = EXCLUDED.model_id,
  enabled     = EXCLUDED.enabled,
  updated_at  = NOW();

INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, priority, enabled)
SELECT 'waybill:assist', 'chat', mm.id, m.id, 100, true
FROM ai_providers mm
JOIN ai_models m ON m.provider_id = mm.id AND m.name = 'MiniMax-M3'
WHERE mm.name = 'MiniMax'
ON CONFLICT (section_key, task_type, priority) DO UPDATE SET
  provider_id = EXCLUDED.provider_id,
  model_id    = EXCLUDED.model_id,
  enabled     = EXCLUDED.enabled,
  updated_at  = NOW();
