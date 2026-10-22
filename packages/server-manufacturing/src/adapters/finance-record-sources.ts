import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** The immutable full owner posting, exact event/version and original command
 * receipt establish production accounting identity. Labels cannot create it. */
const journal=`SELECT posting.company_id,posting.journal_id id FROM manufacturing_order_postings posting
 JOIN manufacturing_orders production ON production.id=posting.order_id AND production.company_id=posting.company_id AND production.version>=posting.source_order_version
 JOIN planning_recommendations recommendation ON recommendation.id=production.recommendation_id
 JOIN planning_scenarios scenario ON scenario.id=recommendation.scenario_id AND scenario.company_id=production.company_id AND scenario.branch_id=production.branch_id AND scenario.warehouse_id=production.warehouse_id AND scenario.specification_id=production.specification_id AND scenario.specification_version=production.specification_version AND scenario.status='released'
 JOIN manufacturing_specifications specification ON specification.id=production.specification_id AND specification.company_id=production.company_id
 JOIN manufacturing_order_events event ON event.order_id=production.id AND event.version=posting.source_order_version AND event.actor_id=posting.created_by AND event.id='mfg-event-'||substring(posting.id FROM length('mfg-posting-')+1)
 JOIN command_receipts receipt ON receipt.command_id=substring(posting.id FROM length('mfg-posting-')+1) AND receipt.record_id=production.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=production.company_id AND receipt.operation='manufacturing.order.'||event.action
 JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id AND profile.version>=posting.profile_version
 JOIN finance_journals journal ON journal.id=posting.journal_id AND journal.company_id=posting.company_id AND journal.chart_id=profile.chart_id AND journal.currency=posting.currency AND journal.currency=profile.currency AND journal.source_type='manufacturing_'||posting.phase AND journal.source_id=production.id AND journal.status='posted'
 WHERE posting.amount_minor>0 AND event.action=CASE posting.phase WHEN 'issue' THEN 'issue' WHEN 'report' THEN 'report' WHEN 'output' THEN 'pass-qc' ELSE 'scrap' END
 AND (event.details_json::jsonb->>'expectedVersion')::bigint=posting.source_order_version-1
 AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=posting.amount_minor AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=posting.amount_minor
 AND (SELECT sum(amount_minor) FROM manufacturing_cost_movements movement WHERE movement.company_id=posting.company_id AND movement.order_id=production.id AND movement.source_type='production_event' AND movement.source_id=event.id AND movement.journal_id=journal.id)=posting.amount_minor`;
export const manufacturingFinanceRecordSources={journal,posting:journal.replace("SELECT posting.company_id,posting.journal_id id","SELECT posting.id posting_id,posting.company_id,posting.journal_id id")} as const;
/** Named relation leases precede the first source snapshot in the complete
 * Finance/Files transaction, including an initially absent posting relation. */
export function manufacturingFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN SHARE MODE"),database.prepare("LOCK TABLE manufacturing_specifications,manufacturing_specification_lines,manufacturing_specification_operations,manufacturing_orders,manufacturing_order_events,manufacturing_accounting_profiles,manufacturing_order_postings,manufacturing_cost_movements IN SHARE MODE")];}
