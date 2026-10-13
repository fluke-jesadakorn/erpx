import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function activeCompanyIdentity(db:SqlDatabase,companyId:string){return db.prepare("SELECT slug,display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'").bind(companyId).first<{slug:string;display_name:string}>();}
export function personLevelHistory(db:SqlDatabase,engagementId:unknown){return db
        .prepare(
          "SELECT h.*,l.name level_name,old.name previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
        )
        .bind(engagementId)
        .all();}
export function leaveDefaultLevels(db:SqlDatabase,companyId:string){return db.prepare("SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number").bind(companyId).all<{id:string;name:string;level_number:number}>();}
export function draftCompanyName(db:SqlDatabase,companyId:string,scopeCompanyId:string){return db.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND company_id=?").bind(companyId,scopeCompanyId).first<{display_name:string}>();}
export function activeCompanySlug(db:SqlDatabase,companyId:string){return db
      .prepare(
        "SELECT slug FROM organization_nodes WHERE id=? AND kind='company' AND status='active'",
      )
      .bind(companyId)
      .first<{ slug: string }>();}
export function activeCompanyName(db:SqlDatabase,companyId:string){return db
        .prepare(
          "SELECT display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'",
        )
        .bind(companyId)
        .first<{ display_name: string }>();}
export function scopedBranchNames(db:SqlDatabase,companyId:string,branchId:string | null,matchingBranchId:string | null){return db
        .prepare(
          "SELECT display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name",
        )
        .bind(companyId, branchId, matchingBranchId)
        .all<{ display_name: string }>();}
