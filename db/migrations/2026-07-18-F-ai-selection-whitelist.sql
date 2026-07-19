-- AI model selection policy.
-- Enabled assignments are the IT-managed section whitelist and priority order.

ALTER TABLE folio.ai_assignments
  ADD COLUMN IF NOT EXISTS user_selectable BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE folio.ai_models
  ADD COLUMN IF NOT EXISTS reasoning_levels TEXT[] NOT NULL DEFAULT '{}',
  ADD COLUMN IF NOT EXISTS is_free BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN IF NOT EXISTS synced_at TIMESTAMP;

INSERT INTO folio.ai_providers (name, type, base_url, enabled, preset, notes)
VALUES ('openrouter', 'openai_compat', 'https://openrouter.ai/api/v1', true, 'openrouter', 'OpenRouter model gateway')
ON CONFLICT (name) DO UPDATE
  SET type = EXCLUDED.type, base_url = EXCLUDED.base_url, preset = EXCLUDED.preset;

CREATE TABLE IF NOT EXISTS folio.ai_user_section_preferences (
  user_id INT NOT NULL REFERENCES folio.users(id) ON DELETE CASCADE,
  section_key TEXT NOT NULL,
  model_id INT NOT NULL REFERENCES folio.ai_models(id) ON DELETE CASCADE,
  think_level TEXT NOT NULL DEFAULT 'auto'
    CHECK (think_level IN ('auto', 'low', 'medium', 'high')),
  updated_at TIMESTAMP NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, section_key)
);
CREATE INDEX IF NOT EXISTS idx_ai_user_section_preferences_section
  ON folio.ai_user_section_preferences(section_key);

DO $$
BEGIN
  IF to_regclass('finance.ai_user_section_defaults') IS NOT NULL THEN
    EXECUTE $sql$
      INSERT INTO folio.ai_user_section_preferences (user_id, section_key, model_id)
      SELECT user_id, section_key, model_id
        FROM finance.ai_user_section_defaults
      ON CONFLICT (user_id, section_key) DO NOTHING
    $sql$;
  END IF;
END $$;

INSERT INTO perm.permissions (id, description) VALUES
  ('ai:preference:read::allow', 'List AI models whitelisted for the current user'),
  ('ai:preference:update::allow', 'Save AI model and thinking preferences'),
  ('ai:model:sync::allow', 'Synchronize provider model metadata')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'ai-selection-whitelist'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE p.id IN ('ai:preference:read::allow', 'ai:preference:update::allow')
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, 'ai:model:sync::allow', 'ai-selection-whitelist'
  FROM perm.roles r
 WHERE r.id IN ('it_manager', 'it_supervisor')
ON CONFLICT DO NOTHING;

UPDATE folio.ai_assignments
   SET user_selectable = true
 WHERE enabled = true
   AND task_type IN ('chat', 'vision');

UPDATE folio.ai_models
   SET reasoning_levels = ARRAY['low', 'medium', 'high']::TEXT[]
 WHERE enabled = true
   AND reasoning_levels = '{}'
   AND name ~* '(qwen3|deepseek|gpt-oss)';

INSERT INTO folio.ai_assignments
  (section_key, task_type, provider_id, model_id, params_json, priority, enabled, user_selectable)
SELECT zones.section_key, 'chat', m.provider_id, m.id, '{}'::jsonb, 100, true, true
  FROM (VALUES
    ('chat:global'), ('chat:hub'), ('chat:expense'), ('chat:sales'), ('chat:customers'),
    ('chat:pr'), ('chat:po'), ('chat:cockpit'), ('chat:executive'), ('chat:ledger'), ('chat:policy'),
    ('chat:tiles'), ('chat:audit'), ('chat:ai-settings'), ('chat:hr'), ('chat:law'),
    ('chat:org'), ('chat:inbox'), ('chat:waybill')
  ) AS zones(section_key)
  CROSS JOIN LATERAL (
    SELECT id, provider_id
      FROM folio.ai_models
     WHERE enabled = true AND 'chat' = ANY(capabilities)
     ORDER BY (name = 'openrouter/free') DESC, is_free DESC, id
     LIMIT 1
  ) m
 WHERE NOT EXISTS (
   SELECT 1 FROM folio.ai_assignments a
    WHERE a.section_key = zones.section_key AND a.task_type = 'chat'
 );

INSERT INTO folio.ai_models
  (provider_id, name, capabilities, context_window, defaults_json, enabled, description, reasoning_levels, is_free, synced_at)
SELECT p.id, 'openrouter/free', ARRAY['chat','vision']::TEXT[], 200000,
       '{"supported_parameters":["frequency_penalty","include_reasoning","logprobs","max_tokens","presence_penalty","reasoning","reasoning_effort","repetition_penalty","response_format","seed","stop","structured_outputs","temperature","tool_choice","tools","top_k","top_logprobs","top_p"],"pricing":{"prompt":"0","completion":"0"}}'::jsonb,
       true, 'OpenRouter free-model router with multimodal request routing.', '{}', true, now()
  FROM folio.ai_providers p
 WHERE p.name = 'openrouter'
ON CONFLICT (provider_id, name) DO UPDATE
  SET capabilities = EXCLUDED.capabilities,
      context_window = EXCLUDED.context_window,
      defaults_json = EXCLUDED.defaults_json,
      enabled = true,
      description = EXCLUDED.description,
      reasoning_levels = EXCLUDED.reasoning_levels,
      is_free = true,
      synced_at = now();

INSERT INTO folio.ai_models
  (provider_id, name, capabilities, context_window, defaults_json, enabled, description, reasoning_levels, is_free, synced_at)
SELECT p.id, 'google/gemma-4-31b-it:free', ARRAY['chat','vision']::TEXT[], 262144,
       '{"supported_parameters":["include_reasoning","max_tokens","reasoning","response_format","seed","temperature","tool_choice","tools","top_p"],"pricing":{"prompt":"0","completion":"0"}}'::jsonb,
       true, 'Google Gemma 4 31B free multimodal model (image/text/video).', '{}', true, now()
  FROM folio.ai_providers p
 WHERE p.name = 'openrouter'
ON CONFLICT (provider_id, name) DO UPDATE
  SET capabilities = EXCLUDED.capabilities,
      context_window = EXCLUDED.context_window,
      defaults_json = EXCLUDED.defaults_json,
      enabled = true,
      description = EXCLUDED.description,
      reasoning_levels = EXCLUDED.reasoning_levels,
      is_free = true,
      synced_at = now();

INSERT INTO folio.ai_assignments
  (section_key, task_type, provider_id, model_id, params_json, priority, enabled, user_selectable)
SELECT z.section_key, z.task_type, m.provider_id, m.id, '{}'::jsonb, z.priority, true, true
  FROM (VALUES
    ('chat:global', 'chat', 110),
    ('staff:ocr', 'vision', 110),
    ('chat:global', 'chat', 120),
    ('staff:ocr', 'vision', 120)
  ) AS z(section_key, task_type, priority)
  JOIN folio.ai_models m
    ON m.name = CASE WHEN z.priority = 110 THEN 'google/gemma-4-31b-it:free' ELSE 'openrouter/free' END
  JOIN folio.ai_providers p ON p.id = m.provider_id AND p.name = 'openrouter'
WHERE NOT EXISTS (
   SELECT 1 FROM folio.ai_assignments a
    WHERE a.section_key = z.section_key
      AND a.task_type = z.task_type
      AND a.priority = z.priority
);

INSERT INTO folio.ai_user_section_preferences (user_id, section_key, model_id, think_level)
SELECT u.id, 'staff:ocr', m.id, 'auto'
  FROM folio.users u
  JOIN folio.ai_models m ON m.name = 'openrouter/free' AND m.enabled = true
 WHERE u.employee_code = 'DEV-IT-OFF'
ON CONFLICT (user_id, section_key) DO UPDATE
   SET model_id = EXCLUDED.model_id,
       think_level = EXCLUDED.think_level,
       updated_at = now();
