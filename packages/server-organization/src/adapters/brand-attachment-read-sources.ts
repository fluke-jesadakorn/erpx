import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Native organization facts for the existing Brand private-file policy.
 * This point port grants neither organization management nor provider access. */
export function brandAttachmentOrganizationSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes IN SHARE ROW EXCLUSIVE MODE')];}
export function brandAttachmentOrganizationSourceImage(database:SqlDatabase,companyId:string,branchId:string|null){
 return database.prepare(`SELECT jsonb_build_object('company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=? AND company.kind='company'),
 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=? AND branch.company_id=? AND branch.kind='branch'))::text image`).bind(companyId,branchId,companyId).first<string>('image');
}
