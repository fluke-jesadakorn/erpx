import 'server-only';
import { query, withTransaction } from '@folio-lib/db';
import { getSemanticSuggestions } from '@folio-lib/waybill/queries';
import { recordEvent } from '@folio-lib/waybill/events';

export interface SoCoaSuggestion {
  itemId: number;
  code: string;
  name: string;
  nameTh: string | null;
  accountType: string;
  normalSide: string;
  similarity: number;
  source: 'semantic' | 'learned' | 'header';
}

export async function suggestSoCoa(soId: number): Promise<SoCoaSuggestion[]> {
  const items = await query<{
    id: number;
    description: string | null;
    mapped_revenue_account_code: string | null;
    confidence_score: number | null;
  }>(
    `SELECT id, description, mapped_revenue_account_code, confidence_score
       FROM so_items
      WHERE sales_order_id = $1
      ORDER BY id`,
    [soId],
  );
  const out: SoCoaSuggestion[] = [];
  for (const it of items.rows) {
    const desc = (it.description ?? '').trim();
    if (!desc) continue;
    const cands = await getSemanticSuggestions(desc);
    if (!cands.success || !cands.suggestions) continue;
    for (const c of cands.suggestions) {
      out.push({
        itemId: it.id,
        code: c.code,
        name: c.name ?? c.code,
        nameTh: c.name_th ?? null,
        accountType: c.account_type ?? 'revenue',
        normalSide: 'credit',
        similarity: c.similarity,
        source: 'semantic',
      });
    }
  }
  return out;
}

export async function applySoCoaAction(input: {
  soId: number;
  itemId: number;
  code: string;
  waybillId: string;
  actorId: number;
  actorPerms: Set<string>;
}): Promise<{ ok: true }> {
  const acct = await query<{ normal_side: string }>(
    `SELECT normal_side FROM chart_of_accounts WHERE code = $1`,
    [input.code],
  );
  if (acct.rows.length === 0) {
    throw new Error(`Unknown COA code ${input.code}`);
  }

  await withTransaction(async (q) => {
    await q(
      `UPDATE so_items
          SET mapped_revenue_account_code = $1
        WHERE id = $2 AND sales_order_id = $3`,
      [input.code, input.itemId, input.soId],
    );
    await recordEvent({
      waybillId: input.waybillId,
      kind: 'coa-applied' as never,
      stageFrom: 'so_paid',
      stageTo: 'so_paid',
      actorId: input.actorId,
      payload: {
        origin: 'so',
        itemId: input.itemId,
        code: input.code,
        normalSide: acct.rows[0].normal_side,
        appliedBy: input.actorId,
      },
      client: q as never,
    });
  });

  return { ok: true };
}

export async function canActAtSalesRecording(
  actorPerms: Set<string>,
  roleName: string,
): Promise<boolean> {
  if (actorPerms.has('admin:system:bypass::allow')) return true;
  if (actorPerms.has('stage:so_paid:act::allow')) return true;
  return roleName === 'finance' || roleName === 'cfo' || roleName === 'admin';
}