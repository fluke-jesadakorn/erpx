import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function stageOrganizationDepartmentPeople(db:SqlDatabase,companyId:string,sourceId:string|null,branchId:string|null,nodeId:string,now:number){return [
db.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND status IN ('draft','onboarding','active','suspended')").bind(branchId,nodeId,now,companyId,sourceId),
          db.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND department_id=? AND status='active'").bind(branchId,nodeId,companyId,sourceId)]; }
