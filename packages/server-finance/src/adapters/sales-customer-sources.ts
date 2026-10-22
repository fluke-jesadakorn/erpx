import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {Row} from '@erpx/server-commerce/public/commerce';
import {readListPage} from '@erpx/server-work_projection/public/list-page';
export function salesPricingCustomers(write:SqlDatabase,actor:WorkContext){return write.prepare("SELECT id,code,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name").bind(actor.companyId).all<Row>();}
export async function salesPricingCustomerValid(write:SqlDatabase,actor:WorkContext,id:string,currency:string){return Boolean(await write.prepare("SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL").bind(id,actor.companyId,currency).first());}
export async function salesCustomerProfile(write:SqlDatabase,actor:WorkContext,id:string){return await write.prepare("SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND person_id IS NULL").bind(id, actor.companyId).first<Row>();}
export async function salesCustomerDirectory(write:SqlDatabase,actor:WorkContext,search:string,status:string,cursor?:string){return await readListPage<Row & { id: string; page_key: string }>(write, {
    sql: "SELECT p.*,p.name page_key FROM finance_parties p WHERE p.company_id=? AND p.kind='customer' AND p.person_id IS NULL AND (?='' OR strpos(lower(p.code||' '||p.name),lower(?))>0) AND (?='' OR p.status=?)",
    args: [actor.companyId, search, search, status, status], cursor: cursor,
    // Retain compatibility with old customer-master pagination when no status is selected.
    scope: [actor.companyId, actor.branchId, "parties", "parties", "customer", search, ...(status ? [status] : [])],
  });}
export function salesNoteCustomer(write:SqlDatabase,a:WorkContext,id:string,version?:number){return write.prepare("SELECT p.id,p.name,p.code,p.version FROM finance_parties p WHERE p.id=? AND p.company_id=? AND p.kind='customer' AND (?::int IS NULL OR p.version=?)").bind(id,a.companyId,version??null,version??null).first<{id:string;name:string;code:string;version:number}>();}

