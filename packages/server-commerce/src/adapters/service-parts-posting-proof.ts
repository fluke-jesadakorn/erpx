import 'server-only';
export const servicePartsPostingCapabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
export const servicePartsPostingMarker='_servicePartsPostingSetup';
/** Existing receipt tables only: pre1062 ordinary readers cannot query an absent
 * native relation. Once installed, reciprocal constraints protect this lineage. */
export const servicePartsPostingSourceRows=`SELECT receipt.command_id,receipt.record_id company_id,receipt.request_hash,receipt.result_json::jsonb->'${servicePartsPostingMarker}' source_proof,receipt.result_json::jsonb->'${servicePartsPostingMarker}'->'mapping' mapping
 FROM command_receipts receipt WHERE to_regclass('commerce_service_parts_posting_sources') IS NOT NULL
 AND receipt.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')
 AND receipt.authority_json::jsonb->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->>'id'=receipt.record_id AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->>'status'=CASE receipt.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END
 AND receipt.result_json::jsonb->'${servicePartsPostingMarker}'->>'kind'='service-parts-posting-setup' AND receipt.result_json::jsonb->'${servicePartsPostingMarker}'->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->'${servicePartsPostingMarker}'->>'commandId'=receipt.command_id AND receipt.result_json::jsonb->'${servicePartsPostingMarker}'->'mapping'->>'company_id'=receipt.record_id`;
