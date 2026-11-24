import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {companyNodeScopeSource} from './organization-read-tail-sources-r39';
/** Company scope stays in the caller's guarded statement and uses the supplied database. */
export function aiGatewayCompanySources(db:SqlDatabase){return {
 scopeFence:`EXISTS(${companyNodeScopeSource})`,
 async scope(companyId:string){return !!await db.prepare(companyNodeScopeSource).bind(companyId,companyId).first();},
};}