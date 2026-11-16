import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export async function lockFinanceStarterCompany(db:SqlDatabase,companyId:string){return db.prepare("SELECT 1").bind().first();}
export async function financeStarterOrganizationNodes(db:SqlDatabase,companyId:string){return (await db.prepare("SELECT id,kind,display_name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','department') AND status='active' ORDER BY kind,id").bind(companyId).all<{id:string;kind:string;display_name:string}>()).results;}
