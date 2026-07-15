-- AI Section Health view: one row per ai_assignments row, with aggregated invocation stats.
-- Used by db/audit_ai_coverage.js and /api/ai/sections/health.

CREATE OR REPLACE VIEW ai_section_health AS
SELECT
  a.id              AS assignment_id,
  a.section_key,
  a.task_type,
  a.enabled         AS assignment_enabled,
  a.priority,
  p.id              AS provider_id,
  p.name            AS provider_name,
  p.type            AS provider_type,
  m.id              AS model_id,
  m.name            AS model_name,
  COUNT(i.id) FILTER (WHERE i.status = 'ok')    AS ok_calls,
  COUNT(i.id) FILTER (WHERE i.status = 'error') AS err_calls,
  COUNT(i.id)                                   AS total_calls,
  MAX(i.created_at)                             AS last_invocation_at,
  MIN(i.created_at)                             AS first_invocation_at
FROM ai_assignments a
LEFT JOIN ai_providers    p ON p.id = a.provider_id
LEFT JOIN ai_models       m ON m.id = a.model_id
LEFT JOIN ai_invocations  i ON i.section_key = a.section_key
GROUP BY a.id, a.section_key, a.task_type, a.enabled, a.priority,
         p.id, p.name, p.type, m.id, m.name;