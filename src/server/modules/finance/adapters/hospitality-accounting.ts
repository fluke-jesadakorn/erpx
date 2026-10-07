import 'server-only';
import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import { pmsAccountKeys, type Snapshot } from "@/server/modules/pms/public/rules/pms-model";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";

export const pmsAccountCategory = (key: string) => ['revenue','service'].includes(key) ? 'revenue' : ['advance','deposit','tax'].includes(key) ? 'liability' : ['consumption','commission','cashDifference'].includes(key) ? 'expense' : 'asset';
export const pmsAccountCode = (key: string) => `PMS-${key.replace(/[A-Z]/g, c => `-${c}`).toUpperCase()}`;
const schema = z.object({
  chartId: z.string().min(1), accounts: z.record(z.string(),z.string()).default({}),
  taxBps: z.number().int().min(0).max(10000), serviceBps: z.number().int().min(0).max(10000),
  taxInclusive: z.boolean(), advanceTax: z.boolean(), financeReviewed: z.literal(true),
}).strict();
/** Called inside the PMS owner transaction; approval remains a separate command. */
export async function preparePmsAccounting(db: SqlDatabase, actor: WorkContext, s: Snapshot, input: unknown) {
  const v = schema.parse(input);
  if (!await db.prepare("SELECT id FROM finance_charts WHERE id=? AND company_id=? AND base_currency='THB' AND status='active' FOR SHARE").bind(v.chartId,actor.companyId).first())
    throw new ApiError(422,'PMS_CHART_INVALID',"Select this company's active THB chart.");
  if (Object.keys(v.accounts).some(k => !pmsAccountKeys.includes(k as typeof pmsAccountKeys[number])))
    throw new ApiError(422,'PMS_ACCOUNT_INVALID','Choose known PMS account categories.');
  const accounts = {} as Snapshot['property']['config']['accounts'];
  for (const key of pmsAccountKeys) {
    const category = pmsAccountCategory(key), code = pmsAccountCode(key);
    const selected = v.accounts[key];
    const existing = await db.prepare(selected
      ? "SELECT id,category,currency,status FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=? FOR SHARE"
      : "SELECT id,category,currency,status FROM finance_accounts WHERE code=? AND chart_id=? AND company_id=? FOR SHARE")
      .bind(selected || code,v.chartId,actor.companyId).first<{id:string;category:string;currency:string;status:string}>();
    if (selected || existing) {
      if (!existing || existing.category !== category || existing.currency !== 'THB' || existing.status !== 'active')
        throw new ApiError(422,'PMS_ACCOUNT_INVALID',`Review the ${key} mapping in Finance; its category and currency must match.`);
      accounts[key] = existing.id;
    } else {
      const id = randomUUID(), now = Date.now();
      await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?)")
        .bind(id,v.chartId,actor.companyId,code,`Hotel ${key}`,category,key==='bank'?1:0,key==='bank'?'bank':'none',now,now).run();
      accounts[key] = id;
    }
  }
  void s;
  return {chartId:v.chartId,accounts,taxBps:v.taxBps,serviceBps:v.serviceBps,taxInclusive:v.taxInclusive,advanceTax:v.advanceTax};
}
