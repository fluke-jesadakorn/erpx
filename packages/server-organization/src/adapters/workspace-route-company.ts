import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Structural routing facts only. Callers must still obtain current account access. */
export async function workspaceRouteCompany(db:SqlDatabase, branchSlug?:string, departmentSlug?:string, companySlug?:string) {
  const args:string[]=[];
  let source="organization_nodes c", where="c.kind='company' AND c.status='active'";
  if(branchSlug){source+=" JOIN organization_nodes b ON b.company_id=c.id AND b.parent_id=c.id AND b.kind='branch' AND b.status='active'";where+=' AND b.slug=?';args.push(branchSlug);}
  if(departmentSlug){source+=` JOIN organization_nodes d ON d.company_id=c.id AND d.kind='department' AND d.status='active' AND ${branchSlug?"d.branch_id=b.id AND d.parent_id=b.id":"d.branch_id IS NULL AND d.parent_id=c.id"}`;where+=' AND d.slug=?';args.push(departmentSlug);}
  if(companySlug){where+=' AND (c.slug=? OR c.id=?)';args.push(companySlug,companySlug);}
  if(!branchSlug&&!departmentSlug&&!companySlug)return null;
  return db.prepare(`SELECT c.id,c.slug FROM ${source} WHERE ${where}`).bind(...args).first<{id:string;slug:string}>();
}
