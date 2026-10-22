import {inboxAudiencePeopleJoins} from '@erpx/server-people_payroll/public/collaboration-inbox-people';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function collaborationInboxSources(db:SqlDatabase){return {async broadcastPermission(role:string,companyId:string,value:"inbox.notice.broadcast"){return db.prepare("SELECT 1 WHERE iam_role_allows(?,?,?)")
        .bind(role,companyId,value).first();},async audienceRecipients(companyId:string,now:number,now2:number,kind:"company" | "branch" | "department",kind2:"company" | "branch" | "department",id:string,id2:string,kind3:"company" | "branch" | "department",id3:string,id4:string){return db.prepare(`SELECT DISTINCT a.user_id FROM assignments a
        ${inboxAudiencePeopleJoins}
        WHERE a.company_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?)
        AND iam_principal_active(a.user_id,a.company_id,?)
        AND (?='company' OR (?='branch' AND (a.scope_node_id=? OR h.branch_id=?))
             OR (?='department' AND (a.scope_node_id=? OR h.department_id=?)))
        ORDER BY a.user_id LIMIT 10001`).bind(companyId,now,now2,kind,kind2,id,id2,
          kind3,id3,id4).all<{user_id:string}>();},async activeRecipients(companyId:string,ids:string[],now:number,now2:number){return db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${ids.map(()=>"?").join(",")})
      AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`)
      .bind(companyId,...ids,now,now2).all<{user_id:string}>();},async activeFanoutRecipient(company_id:string,user_id:string,now:number,now2:number){return db.prepare(`SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL
      AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) LIMIT 1`)
      .bind(company_id,user_id,now,now2).first();}};}
