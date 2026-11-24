import "server-only";
import {inventoryReadProjection as inventorypeople_payrollProjection} from '@erpx/server-people_payroll/public/inventory-read-projections';
/** Byte-identical closed audit-history projection shared by inventory count and transfer owner sources. The join fragment is fixed; callers retain their binds, filters and result types and supply no SQL or relation name. */
export const inventoryAuditHistorySql = `SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM audit_events a ${inventorypeople_payrollProjection.historyActor} WHERE a.record_id=? ORDER BY a.created_at DESC`;
