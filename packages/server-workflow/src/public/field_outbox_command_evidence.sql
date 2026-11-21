e.aggregate_version BETWEEN 1 AND 9007199254740991
AND e.payload_json::jsonb->>'id'=e.aggregate_id
AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
AND (SELECT count(*) FROM outbox_events all_events WHERE all_events.command_id=e.command_id)=1
AND EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=e.command_id
 AND r.operation=e.event_type AND r.record_id=e.aggregate_id
 AND (CASE WHEN r.authority_json IS JSON OBJECT THEN r.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=e.payload_json::jsonb->>'companyId'
 AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'id'=e.aggregate_id
 AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->'version'=to_jsonb(e.aggregate_version)
 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=r.command_id)=1
 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=r.command_id AND a.actor_id=r.actor_id
  AND a.action=e.event_type AND a.record_type=e.aggregate_type AND a.record_id=e.aggregate_id AND a.record_version=e.aggregate_version
  AND a.authority_json=r.authority_json AND a.created_at=r.created_at AND e.created_at=r.created_at))
