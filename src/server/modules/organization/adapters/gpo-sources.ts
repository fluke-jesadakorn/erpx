import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {Position} from '../../identity_access/public/gpo-contract';
import type {GpoRuleTarget} from "@/lib/organization/gpo-rules";
/** All facts use the supplied transaction; Company and placement scope remain explicit. */
export async function gpoVisiblePositions(db:SqlDatabase,companyId:string,branchId:string | null,departmentId:string | null){return (await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1 AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)")
    .bind(companyId, branchId, branchId, departmentId, departmentId).all<{ id: string }>()).results;}
export async function gpoTargetName(db:SqlDatabase,companyId:string,target:GpoRuleTarget){return db.prepare(target.kind === "level" ? "SELECT name FROM staff_levels WHERE company_id=? AND id=? AND active=1 AND level_number IS NOT NULL" : target.kind === "department" ? "SELECT display_name name FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active'" : "SELECT title name FROM organization_positions WHERE company_id=? AND id=? AND active=1").bind(companyId,target.id).first<{name:string}>();}
export async function gpoActivePositions(db:SqlDatabase,companyId:string){return (await db.prepare("SELECT id,title,branch_id,department_id,level_id,version FROM organization_positions WHERE company_id=? AND active=1 ORDER BY id").bind(companyId).all<Position>()).results;}
export async function gpoActivePosition(db:SqlDatabase,positionId:string,companyId:string){return db.prepare("SELECT id,title FROM organization_positions WHERE id=? AND company_id=? AND active=1").bind(positionId,companyId).first<{id:string;title:string}>();}
export async function gpoOverviewPositions(db:SqlDatabase,companyId:string){return (await db.prepare(`SELECT o.id,o.title,COUNT(DISTINCT e.id)::integer holder_count,p.version mapping_version,
      p.connection_id IS NOT NULL mapped,CASE WHEN p.connection_id IS NOT NULL THEN p.directory_config_status ELSE NULL END mapping_status
      FROM organization_positions o LEFT JOIN (${identity_accessPlatformSources.position_gpo_policies}) p ON p.company_id=o.company_id AND p.position_id=o.id
      LEFT JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.company_id=o.company_id AND e.position_id=o.id AND e.status='active'
        AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      WHERE o.company_id=? AND o.active=1 GROUP BY o.id,o.title,p.version,p.connection_id,p.directory_config_status ORDER BY o.title,o.id`).bind(companyId).all()).results;}
