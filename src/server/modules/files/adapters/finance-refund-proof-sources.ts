import 'server-only';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
export type RefundProofSource = {
    id: string;
    purpose_key: string;
    component_kind: string;
    component_ref: string | null;
};
export async function refundProofSets(db: SqlDatabase, actor: WorkContext, ids: string[]): Promise<RefundProofSource[]> { const placeholders = ids.map(() => '?').join(','); return (await db.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(actor.companyId, ...ids).all<RefundProofSource>()).results; }
