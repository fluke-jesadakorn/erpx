import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){return {prepareRevocations(month:number,user:string,companyId:string){return db
                .prepare(
                  "UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL",
                )
                .bind(month, user, companyId);}};}
