import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
type Row = Record<string, string | number | null>;
export function qualityProductFacts(write: SqlDatabase, actor: Pick<WorkContext, "companyId">, id: string) { return write.prepare("SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'").bind(id, actor.companyId).first<Row>(); }
