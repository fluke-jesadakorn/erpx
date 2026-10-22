import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function stripeEntrypointFacts(db:SqlDatabase){return {
 connection:(companyId:string)=>db.prepare('SELECT * FROM pos_connections WHERE company_id=?').bind(companyId).first(),
 jobs:(companyId:string)=>db.prepare("SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10").bind(companyId).all()
};}
