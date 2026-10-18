import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Exact immutable part consumption, its real Work/physical Asset and original
 * draft/post journal establish ownership. A source label alone grants nothing. */
export const maintenanceFinanceRecordSources={journal:`SELECT consumption.company_id,consumption.journal_id id
 FROM operational_inventory_consumptions consumption
 JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id AND work.branch_id=consumption.branch_id AND work.department_id IS NOT DISTINCT FROM consumption.department_id
 JOIN operational_assets asset ON asset.id=work.asset_id AND asset.company_id=work.company_id AND asset.branch_id=work.branch_id AND asset.department_id IS NOT DISTINCT FROM work.department_id
 JOIN finance_journals journal ON journal.id=consumption.journal_id AND journal.company_id=consumption.company_id AND journal.source_type='maintenance_parts' AND journal.source_id=consumption.id AND journal.currency=consumption.currency
 WHERE consumption.source_kind='maintenance' AND consumption.source_version<=work.version AND consumption.quantity_milli>0 AND consumption.value_minor>0
 AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor`} as const;
export function maintenanceFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE")];}
