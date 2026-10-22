import {assistantPage,type AssistantSource} from "@erpx/feature-work_projection/lib/ai/workspace-assistant";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {BusinessRuleError} from '@erpx/server-shared/index';
export type Row = { id:string; question:string; paths_json:string; sources_json:string|null; answer_json:string|null; english_answer_json?:string|null; state:string; created_at:number; private_session_id?:string|null; governance_snapshot_json?:string|null };
export function page(path:string) {
  try { return assistantPage(path); } catch { throw new BusinessRuleError(422,"AI_CONTEXT_UNSUPPORTED","This page is not available to the assistant yet."); }
}
export const scopeKey = (actor:WorkContext) => JSON.stringify([actor.holdingId,actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId,actor.iam?.roleRevision,actor.iam?.identityVersion]);
export function conversation(actor:WorkContext,path:string,mode:"page"|"workspace") {
  if(mode==="workspace"&&!hasPermission(actor,"ai.context.cross")) throw new BusinessRuleError(403,"AI_CONTEXT_FORBIDDEN","Your current role allows one page at a time.");
  return mode==="workspace"?"workspace":page(path).path;
}
export const pick = (row:Record<string,unknown>,keys:string[]) => Object.fromEntries(keys.filter(k=>Object.hasOwn(row,k)).map(k=>[k,row[k]]));
export const sourceReference = ({id,title,path,version,source}:AssistantSource) => ({id,title,path,version,...(source?{source}:{})});

