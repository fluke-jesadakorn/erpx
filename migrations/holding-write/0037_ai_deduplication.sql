CREATE UNIQUE INDEX expense_ai_source_active ON expense_ai_proposals(company_id,expense_id,COALESCE(payment_draft_id,''),stage,source_version,evidence_digest,provider_json)
 WHERE state IN ('queued','running','ready','applied');
