-- Per-user model default per AI section.
-- When set, the router prefers this row over the global section assignment.

CREATE TABLE IF NOT EXISTS ai_user_section_defaults (
  user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  section_key TEXT NOT NULL,
  model_id INT NOT NULL REFERENCES ai_models(id) ON DELETE CASCADE,
  updated_at TIMESTAMP NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, section_key)
);
CREATE INDEX IF NOT EXISTS idx_ai_user_section_defaults_section
  ON ai_user_section_defaults(section_key);

-- Backfill missing section assignments so every catalog section has at least one
-- enabled row pointing at MiniMax-M3 (provider 13, model 27).
-- Existing rows are left untouched thanks to the unique (section_key, task_type, priority) constraint.

INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, params_json, priority, enabled)
SELECT v.section_key, v.task_type, 13 AS provider_id, 27 AS model_id, '{}'::jsonb, 200 AS priority, true
FROM (VALUES
  ('chat:full'::text, 'chat'::text),
  ('tile:explainer', 'chat'),
  ('events:explain', 'chat'),
  ('am:recommend', 'chat'),
  ('hr:agent', 'chat'),
  ('command:intent', 'chat'),
  ('hod:approve', 'chat'),
  ('am:review', 'chat'),
  ('cfo:cockpit', 'chat'),
  ('ceo:cockpit', 'chat'),
  ('manager:approve', 'chat'),
  ('staff:submit', 'chat'),
  ('policy:editor', 'chat'),
  ('ledger:commentary', 'chat'),
  ('notification:digest', 'chat'),
  ('waybill:assist', 'chat'),
  ('sales:extract', 'chat'),
  ('customer:advisory', 'chat'),
  ('staff:ocr', 'vision'),
  ('waybill:settle', 'vision'),
  ('acct:coa-search', 'embed'),
  ('acct:queue', 'chat'),
  ('finance:rag', 'chat'),
  ('cockpit:projection', 'chat'),
  ('cockpit:summarize', 'chat'),
  ('cockpit:sql', 'chat')
) AS v(section_key, task_type)
WHERE NOT EXISTS (
  SELECT 1 FROM ai_assignments a
  WHERE a.section_key = v.section_key AND a.task_type = v.task_type AND a.enabled = true
);
