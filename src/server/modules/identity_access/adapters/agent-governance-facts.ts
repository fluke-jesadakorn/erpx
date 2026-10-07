import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import {agentMemberNameProjection} from "../../people_payroll/public/agent-member-name-projection";
export async function agentGovernanceEligiblePeople(db:SqlDatabase,companyId:string,now:number){return (await db.prepare(`SELECT DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name FROM assignments x ${agentMemberNameProjection()} WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2`).bind(companyId,now,now).all<{id:string;name:string}>()).results;}
export function agentGovernanceEligibleMember(db:SqlDatabase,key:string,label:string,companyId:string,userId:string,now:number){return assertion(db,key,label,"EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?))",[companyId,userId,now,now]);}
