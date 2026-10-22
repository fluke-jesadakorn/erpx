import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
type Row = Record<string, string | number | null>;
export function qualityWorkspaceStock(write: SqlDatabase, actor: WorkContext) { return write.prepare(`SELECT s.warehouse_id,s.product_id,s.quantity_milli,s.version,p.name product_name,p.tracking_mode,w.name warehouse_name FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_products p ON p.id=s.product_id JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND p.company_id=w.company_id AND w.status='active' AND p.status='active' AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name,p.name`).bind(actor.companyId, actor.branchId, actor.branchId).all<Row>(); }
