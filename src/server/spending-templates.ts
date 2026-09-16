import { requirePermission } from "@/server/iam-permissions";
import { z } from "zod";
import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand } from "./commands";
import { ApiError } from "./http";
import type { SpendingTemplate } from "@/lib/spending";

export const spendingTemplateSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(1000),
  exceptions: z.array(z.object({
    positionId: z.string().min(1).nullable(), requestKind: z.enum(["expense","purchase","payment"]).nullable(),
    action: z.enum(["skip_supervisor","named_supervisor"]), supervisorUserId: z.string().min(1).nullable(),
    reason: z.string().trim().min(3).max(1000),
  })).max(100),
});
export async function spendingTemplate(db: SqlDatabase, companyId: string): Promise<SpendingTemplate> {
  const row = await db.prepare("SELECT version,exceptions_json,reason FROM spending_approval_templates WHERE company_id=?").bind(companyId).first<{version:number;exceptions_json:string;reason:string}>();
  return row ? {version:row.version, exceptions:JSON.parse(row.exceptions_json),reason:row.reason} : {version:1,exceptions:[],reason:"Company default: reporting supervisor, Accounting, posting and Finance payment."};
}
export async function saveSpendingTemplate(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof spendingTemplateSchema>, key:string) {
  actor=requirePermission(actor,"organization.manage");
  if(actor.assignmentScope!=="holding") throw new ApiError(403,"COMPANY_AUTHORITY_REQUIRED","Company-authorized HR or an owner must manage spending templates.");
  const seen=new Set<string>();
  for(const exception of input.exceptions){
    const selector=JSON.stringify([exception.positionId,exception.requestKind]);
    if(seen.has(selector)||(!exception.positionId&&!exception.requestKind))throw new ApiError(422,"EXCEPTION_SELECTOR_REQUIRED","Use a unique position or request type for each exception.");
    seen.add(selector);
    if(exception.positionId&&!await db.prepare("SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND active=1").bind(exception.positionId,actor.companyId).first())throw new ApiError(422,"POSITION_INVALID","Choose an active company position.");
    if(exception.action==="named_supervisor"&&!exception.supervisorUserId)throw new ApiError(422,"SUPERVISOR_REQUIRED","Choose an independent supervisor for this exception.");
  }
  const version=input.expectedVersion+1;
  return executeCommand(db,actor,key,"spending.template.save",input,actor.companyId,version,{id:actor.companyId,version},[
    assertion(db,key,"template-version","COALESCE((SELECT version FROM spending_approval_templates WHERE company_id=?),1)=?",[actor.companyId,input.expectedVersion]),
    db.prepare("INSERT INTO spending_approval_templates(company_id,version,exceptions_json,reason,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,exceptions_json=excluded.exceptions_json,reason=excluded.reason,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(actor.companyId,version,JSON.stringify(input.exceptions),input.reason,actor.userId,Date.now()),
  ]);
}
