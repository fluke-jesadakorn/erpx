-- 2026-07-12-A: register MiniMax-VL-01 vision model under the MiniMax provider.
-- The /expense receipt-extraction picker reads from ai_models WHERE capabilities
-- contains 'vision' — adding VL-01 gives users a second MiniMax option alongside
-- MiniMax-M3. Idempotent: re-runs of seed_ai_settings.js produce the same result.

INSERT INTO ai_models (provider_id, name, capabilities, context_window, defaults_json)
SELECT id, 'MiniMax-VL-01', ARRAY['chat','vision'], 64000, '{"temperature": 0.1, "max_tokens": 1024}'::jsonb
FROM ai_providers WHERE name = 'MiniMax'
ON CONFLICT (provider_id, name) DO UPDATE SET
  capabilities = EXCLUDED.capabilities,
  context_window = EXCLUDED.context_window,
  defaults_json = EXCLUDED.defaults_json,
  enabled = true;

UPDATE ai_models
SET description = 'MiniMax VL-01 — dedicated vision variant. Higher accuracy on receipts + tables, lower temperature for deterministic extraction.'
WHERE name = 'MiniMax-VL-01' AND description IS NULL;
