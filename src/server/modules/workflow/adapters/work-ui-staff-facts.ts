import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function workUiStaffFacts(db:SqlDatabase){return {
async approvalQueue(companyId:string,value:0 | 1){return db
              .prepare(
                "SELECT a.*,s.approver_user_id FROM approval_instances a LEFT JOIN approval_steps s ON s.instance_id=a.id AND s.ordinal=a.current_step WHERE a.company_id=? AND (?=0 OR a.record_type='expense') AND a.state IN ('pending','blocked') ORDER BY a.created_at LIMIT 250",
              )
              .bind(companyId, value)
              .all();},
async betaApproval(value:string,companyId:string){return db
              .prepare(
                "SELECT 1 FROM approval_instances WHERE id=? AND company_id=? AND record_type='expense'",
              )
              .bind(value, companyId)
              .first();}
};}
