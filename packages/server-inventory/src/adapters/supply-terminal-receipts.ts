import "server-only";
import {recordAttachmentVersionSteps} from "@erpx/server-files/public/record-version-receipts";

/** Closed owner facts: only a normal terminal decision followed by contiguous
 * actual Files commands can explain a later Count/Landed Cost version. A later
 * version number or a terminal status label alone never establishes history. */
export const supplyInventoryTerminalReceipts=`WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)`;
