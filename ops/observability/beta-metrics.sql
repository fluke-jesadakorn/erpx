-- Execute per Holding write database. These are experiment indicators, not proof of demand.
SELECT p.company_id,
 (SELECT count(*) FROM hr_people h WHERE h.company_id=p.company_id AND h.status='active' AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=h.user_id AND a.company_id=h.company_id AND a.revoked_at IS NULL AND a.valid_until>unixepoch()*1000)) AS people,
 count(t.id) AS completed_post_tasks,
 sum(CASE WHEN t.updated_at<=p.created_at+7*86400000 THEN 1 ELSE 0 END) AS completed_within_first_week
FROM beta_policy p LEFT JOIN (staff_tasks t JOIN social_task_origins o ON o.task_id=t.id)
 ON t.company_id=p.company_id AND t.state='completed' GROUP BY p.company_id;

SELECT strftime('%Y-%W',created_at/1000,'unixepoch') week,count(DISTINCT actor_id) active_people,count(*) actions
FROM audit_events WHERE action LIKE 'social.%' OR action LIKE 'task.%' GROUP BY week ORDER BY week;

SELECT company_id,month,count(*) requests,sum(input_tokens) input_tokens,sum(output_tokens) output_tokens
FROM social_ai_usage GROUP BY company_id,month;

SELECT count(*) files,sum(bytes) stored_bytes FROM attachments WHERE revoked_at IS NULL;
-- Provider invoices, hosting/email charges, support time, manager interviews,
-- offered prices and explicit paid-pilot agreements are recorded by the operator.
