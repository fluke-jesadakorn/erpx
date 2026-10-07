import 'server-only';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
type Row = Record<string, string | number | null>;
export function qualityWorkspaceProducts(write: SqlDatabase, actor: WorkContext) { return write.prepare("SELECT id,sku,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku,name").bind(actor.companyId).all<Row>(); }
