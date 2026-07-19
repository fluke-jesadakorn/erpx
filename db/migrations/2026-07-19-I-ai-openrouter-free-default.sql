INSERT INTO folio.ai_providers (name, type, base_url, enabled, preset, notes)
VALUES ('openrouter', 'openai_compat', 'https://openrouter.ai/api/v1', true, 'openrouter', 'OpenRouter free-model router')
ON CONFLICT (name) DO UPDATE
   SET type = EXCLUDED.type,
       base_url = EXCLUDED.base_url,
       enabled = true,
       preset = EXCLUDED.preset;

INSERT INTO folio.ai_models
  (provider_id, name, capabilities, context_window, defaults_json, enabled, description, is_free, synced_at)
SELECT p.id, 'openrouter/free', ARRAY['chat', 'vision']::TEXT[], 200000,
       '{"temperature": 0.3}'::jsonb, true,
       'OpenRouter free-model router with multimodal request routing.', true, now()
  FROM folio.ai_providers p
 WHERE p.name = 'openrouter'
ON CONFLICT (provider_id, name) DO UPDATE
   SET capabilities = EXCLUDED.capabilities,
       context_window = EXCLUDED.context_window,
       enabled = true,
       description = EXCLUDED.description,
       is_free = true,
       synced_at = now();

UPDATE folio.ai_assignments a
   SET provider_id = free.provider_id,
       model_id = free.id,
       user_selectable = true,
       updated_at = now()
  FROM folio.ai_models current_model
  JOIN folio.ai_providers current_provider ON current_provider.id = current_model.provider_id
  CROSS JOIN folio.ai_models free
  JOIN folio.ai_providers free_provider ON free_provider.id = free.provider_id
 WHERE a.model_id = current_model.id
   AND a.task_type IN ('chat', 'vision')
   AND current_provider.name IN ('MiniMax', 'local-ollama')
   AND free_provider.name = 'openrouter'
   AND free.name = 'openrouter/free'
   AND free.enabled = true;

UPDATE folio.ai_assignments a
   SET provider_id = free.provider_id,
       model_id = free.id,
       user_selectable = true,
       updated_at = now()
  FROM folio.ai_models free
  JOIN folio.ai_providers free_provider ON free_provider.id = free.provider_id
 WHERE a.task_type IN ('chat', 'vision')
   AND a.provider_id IS NULL
   AND a.model_id IS NOT NULL
   AND free_provider.name = 'openrouter'
   AND free.name = 'openrouter/free'
   AND free.enabled = true;

INSERT INTO folio.ai_assignments
  (section_key, task_type, provider_id, model_id, params_json, priority, enabled, user_selectable)
SELECT sections.section_key, 'chat', free.provider_id, free.id, '{}'::jsonb, 100, true, true
  FROM (VALUES
    ('chat:global'), ('chat:hub'), ('chat:expense'), ('chat:sales'), ('chat:customers'),
    ('chat:pr'), ('chat:po'), ('chat:cockpit'), ('chat:executive'), ('chat:ledger'), ('chat:policy'),
    ('chat:tiles'), ('chat:audit'), ('chat:ai-settings'), ('chat:hr'), ('chat:law'),
    ('chat:org'), ('chat:inbox'), ('chat:waybill'), ('hr:classify-intent'),
    ('customer:credit-check'), ('am:recommend'), ('manager:approve'), ('waybill:assist'),
    ('staff-test:accountant-reviewer'), ('staff:submit'), ('acct:queue'), ('hod:approve'),
    ('am:review'), ('cfo:cockpit'), ('ceo:cockpit'), ('ledger:commentary'), ('policy:editor'),
    ('command:intent'), ('notification:digest'), ('sales:extract'), ('customer:advisory'),
    ('cockpit:sql'), ('cockpit:projection'), ('cockpit:summarize'), ('finance:rag'),
    ('hr:agent'), ('chat:full'), ('events:explain'), ('law:rag')
  ) AS sections(section_key)
  CROSS JOIN folio.ai_models free
  JOIN folio.ai_providers free_provider ON free_provider.id = free.provider_id
 WHERE free_provider.name = 'openrouter'
   AND free.name = 'openrouter/free'
   AND free.enabled = true
   AND NOT EXISTS (
     SELECT 1
       FROM folio.ai_assignments existing
      WHERE existing.section_key = sections.section_key
        AND existing.task_type = 'chat'
        AND existing.priority = 100
   );

UPDATE folio.ai_staff s
   SET default_provider_id = free.provider_id,
       default_model_id = free.id
  FROM folio.ai_models current_model
  JOIN folio.ai_providers current_provider ON current_provider.id = current_model.provider_id
  CROSS JOIN folio.ai_models free
  JOIN folio.ai_providers free_provider ON free_provider.id = free.provider_id
 WHERE s.default_model_id = current_model.id
   AND current_provider.name IN ('MiniMax', 'local-ollama')
   AND free_provider.name = 'openrouter'
   AND free.name = 'openrouter/free'
   AND free.enabled = true;

UPDATE folio.ai_user_section_preferences pref
   SET model_id = free.id,
       updated_at = now()
  FROM folio.ai_models current_model
  JOIN folio.ai_providers current_provider ON current_provider.id = current_model.provider_id
  CROSS JOIN folio.ai_models free
  JOIN folio.ai_providers free_provider ON free_provider.id = free.provider_id
 WHERE pref.model_id = current_model.id
   AND current_provider.name = 'MiniMax'
   AND free_provider.name = 'openrouter'
   AND free.name = 'openrouter/free'
   AND free.enabled = true;

ALTER TABLE chat.sessions
  ALTER COLUMN model_name SET DEFAULT 'openrouter/free';
