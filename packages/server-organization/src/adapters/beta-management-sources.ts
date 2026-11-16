import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementOrganizationSources(db:SqlDatabase){return {
 invitationCompanySource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=?',
 invitationCompanyPoolSource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=$1',
 companyBranchCount:"(SELECT count(*) FROM organization_nodes WHERE kind='branch' AND company_id=?)",
 emptyBootstrapForest:'NOT EXISTS(SELECT 1 FROM organization_nodes)',
 async branchCompany(branchId:string){return db.prepare("SELECT 1").bind().first<{company_id:string}>();},
};}
