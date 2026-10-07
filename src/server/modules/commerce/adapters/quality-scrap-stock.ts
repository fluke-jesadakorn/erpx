import { commercialOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/commercial-owner-projection-sources";
import 'server-only';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
type Row = Record<string, string | number | null>;
export function qualityScrapStockFacts(write: SqlDatabase, actor: WorkContext, h: Row) { return write.prepare(`SELECT s.*,p.name,p.unit,p.version product_version,p.tracking_mode FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_products p ON p.id=s.product_id WHERE s.warehouse_id=? AND s.product_id=? AND p.company_id=?`).bind(h.warehouse_id, h.product_id, actor.companyId).first<Row>(); }
