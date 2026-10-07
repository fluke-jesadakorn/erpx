import {recordPeoplePlacementLevelHistory} from "../../organization/public/people-placement-level-history";
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {ClaimedPositionMove,MoveMember} from '../../organization/public/position-move-contract';
import type {PositionMoveInput,PositionMovePreview} from '@/lib/position-moves';
export async function applyOrganizationPositionMoveHolder(db:SqlDatabase,row:ClaimedPositionMove,input:PositionMoveInput,holder:PositionMovePreview['people'][number],original:MoveMember,now:number){
          await db.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,level_id=?,manager_person_id=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
            .bind(holder.after.branchId, holder.after.departmentId, holder.after.levelId, holder.after.managerPersonId, now, row.company_id, holder.engagementId).run();
          // Employment status, start/end dates, salary and pay group are deliberately preserved.
          await db.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?").bind(holder.after.branchId, holder.after.departmentId, row.company_id, holder.personId).run();
          await db.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(`position-move-history-${row.id}-${holder.engagementId}`, row.company_id, holder.engagementId, JSON.stringify({ ...holder.before, status: original.status, startDate: original.start_date, endDate: original.end_date }), JSON.stringify({ ...holder.after, status: original.status, startDate: original.start_date, endDate: original.end_date, effectiveAt: row.effective_at }), row.created_by, input.reason, row.id, now).run();

 if(holder.before.levelId!==holder.after.levelId){
            await recordPeoplePlacementLevelHistory(db,`position-move-level-${row.id}-${holder.engagementId}`,holder.engagementId,holder.before.levelId,holder.after.levelId,row.created_by,input.reason,row.id,now);

 }}
export async function organizationPositionMoveMembers(db:SqlDatabase,companyId:string,positionId:string){return (await db.prepare("SELECT id,person_id,version,status,start_date,end_date,currency FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended' ORDER BY id").bind(companyId, positionId).all<{ id: string; person_id: string; version: number; status: string; start_date: string; end_date: string | null; currency: string }>()).results;}
export async function organizationPositionMovePlacements(db:SqlDatabase,companyId:string,positionId:string){return (await db.prepare(`SELECT c.id FROM people_lifecycle_cases c WHERE c.company_id=? AND c.status IN ('waiting_it','pending_review','scheduled') AND
    (c.definition->>'positionId'=? OR c.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended')
      OR c.person_id IN (SELECT person_id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended')) ORDER BY c.id`)
    .bind(companyId, positionId, companyId, positionId, companyId, positionId).all<{ id: string }>()).results;}
export async function organizationPositionMoveLeave(db:SqlDatabase,companyId:string,positionId:string,date:string){return (await db.prepare(`SELECT a.engagement_id,a.id,a.effective_from,a.effective_to,a.version,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id
    WHERE a.company_id=? AND a.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status!='ended') AND (a.effective_to IS NULL OR a.effective_to>=?) ORDER BY a.engagement_id,a.effective_from DESC,a.id`)
    .bind(companyId, companyId, positionId, date).all<import('../../organization/public/position-move-contract').Leave>()).results;}
