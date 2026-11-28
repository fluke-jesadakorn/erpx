import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ORG_BRANCH_COMPANY_SOURCE} from './org-sources';
import {notExistsOrgNodeSql} from './organization-read-tail-sources-r56';
export function betaManagementOrganizationSources(db:SqlDatabase){return {
 invitationCompanySource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=?',
 invitationCompanyPoolSource:'JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=$1',
 companyBranchCount:"(SELECT count(*) FROM organization_nodes WHERE kind='branch' AND company_id=?)",
 emptyBootstrapForest:notExistsOrgNodeSql(''),
 async branchCompany(branchId:string){return db.prepare(ORG_BRANCH_COMPANY_SOURCE).bind(branchId).first<{company_id:string}>();},
};}
