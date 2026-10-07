import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function betaManagementOrganizationSources(db:SqlDatabase){return {
 invitationCompanySource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=?',
 invitationCompanyPoolSource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=$1',
 companyBranchCount:"(SELECT count(*) FROM organization_nodes WHERE kind='branch' AND company_id=?)",
 emptyBootstrapForest:'NOT EXISTS(SELECT 1 FROM organization_nodes)',
 async branchCompany(branchId:string){return db.prepare("SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'").bind(branchId).first<{company_id:string}>();},
};}
