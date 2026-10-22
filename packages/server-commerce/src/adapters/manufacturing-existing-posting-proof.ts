import 'server-only';
/** Existing-table-only identity for readers before1060. The additive native
 * reciprocal constraints guarantee a committed private confirmation has its
 * immutable source row. This never relabels an initialize receipt. */
export const manufacturingExistingPostingMarker='_manufacturingExistingPostingSetup';
export const manufacturingPostingSetupReceiptRows=`SELECT r.* FROM command_receipts r WHERE
 (r.operation='commerce.manufacturing-posting-setup.initialize' AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='configured' AND r.result_json::jsonb->>'version'='1' AND r.authority_json::jsonb->>'companyId'=r.record_id)
 OR (r.operation='commerce.manufacturing-posting-setup.confirm-existing' AND jsonb_exists(r.result_json::jsonb,'${manufacturingExistingPostingMarker}')
 AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='confirmed-existing' AND r.result_json::jsonb->>'version'='1'
 AND r.result_json::jsonb->'${manufacturingExistingPostingMarker}'->>'kind'='manufacturing-existing-posting-setup'
 AND r.result_json::jsonb->'${manufacturingExistingPostingMarker}'->>'companyId'=r.record_id
 AND r.result_json::jsonb->'${manufacturingExistingPostingMarker}'->>'confirmationCommandId'=r.command_id
 AND r.result_json::jsonb->'${manufacturingExistingPostingMarker}'->'mapping'->>'company_id'=r.record_id)`;
