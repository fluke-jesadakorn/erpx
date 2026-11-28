import 'server-only';
import type {SqlDatabase,SqlResult} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeOrgAll} from './native-organization-owner-write';
import { levelHistorySql } from './organization-read-tail-sources-r60';

/**
 * Draft/presentation company-identity reads are native-owned in the
 * Organization owner crate. No SQL copy remains here: each function sends only
 * the allow-listed kind and Company-keyed values. Permission checks stay in the
 * caller; an unconfigured or failing native service fails closed.
 */
const asResult = <T>(rows:T[]):SqlResult<T>=>({results:rows,success:true,meta:{changes:0,duration:0}});
export function activeCompanyIdentity(db:SqlDatabase,companyId:string){return nativeOrgAll<{slug:string;display_name:string}>(db,companyId,'presentation_active_company_identity',[companyId]).then(rows=>rows[0]??null);}
export function personLevelHistory(db:SqlDatabase,engagementId:unknown){return db
        .prepare(
          levelHistorySql(",old.name previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC"),
        )
        .bind(engagementId)
        .all();}
export function leaveDefaultLevels(db:SqlDatabase,companyId:string){return nativeOrgAll<{id:string;name:string;level_number:number}>(db,companyId,'presentation_leave_default_levels',[companyId]).then(asResult);}
export function draftCompanyName(db:SqlDatabase,companyId:string,scopeCompanyId:string){return nativeOrgAll<{display_name:string}>(db,companyId,'presentation_draft_company_name',[companyId,scopeCompanyId]).then(rows=>rows[0]??null);}
export function activeCompanySlug(db:SqlDatabase,companyId:string){return nativeOrgAll<{slug:string}>(db,companyId,'presentation_active_company_slug',[companyId]).then(rows=>rows[0]??null);}
export function activeCompanyName(db:SqlDatabase,companyId:string){return nativeOrgAll<{display_name:string}>(db,companyId,'presentation_active_company_name',[companyId]).then(rows=>rows[0]??null);}
export function scopedBranchNames(db:SqlDatabase,companyId:string,branchId:string | null,matchingBranchId:string | null){return nativeOrgAll<{display_name:string}>(db,companyId,'presentation_scoped_branch_names',[companyId,branchId,matchingBranchId]).then(asResult);}
