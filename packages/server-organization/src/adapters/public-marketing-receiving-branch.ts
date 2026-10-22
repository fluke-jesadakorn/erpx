import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function publicMarketingReceivingBranch(db:SqlDatabase,companyId:string){return db.prepare("SELECT id FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' ORDER BY id LIMIT 1").bind(companyId).first<{id:string}>();}
