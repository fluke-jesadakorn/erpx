import { nativeMarketingStatement } from '@erpx/feature-marketing/adapters/server/native-marketing-command';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function prepareCrmSourceTouch(db:SqlDatabase,a:{companyId:string;userId:string},input:{id:string;key:string;campaignId?:string;source:string;sourceEvidence?:string;sourceKind?:string;sourceId?:string;now:number}){
 return nativeMarketingStatement('marketing_stmt_5', { companyId: a.companyId, now: Date.now(), actor: undefined }).bind(`touch-${input.key}`,a.companyId,input.id,input.campaignId??null,input.source,input.sourceEvidence??input.source,input.sourceKind??"staff",input.sourceId??input.id,input.now,a.userId);
}
