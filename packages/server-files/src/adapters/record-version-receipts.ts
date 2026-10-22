import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Historical owner-version steps from the original Files command, immutable
 * binding event and its exact set. A target label or later set state alone
 * cannot prove that a business owner's terminal version legitimately advanced. */
export const recordAttachmentVersionSteps=`SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))`;
export function recordAttachmentVersionSourceReadLocks(database:SqlDatabase){
 return [database.prepare("LOCK TABLE attachment_sets,attachment_events,command_receipts IN SHARE MODE")];
}
