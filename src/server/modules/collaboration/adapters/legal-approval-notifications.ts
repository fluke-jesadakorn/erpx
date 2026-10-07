import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function legalApprovalNotifications(db:SqlDatabase){return {
notifyReviewer:(facts:{identifier:string;companyId:string;approver_user_id:string;itemId:string;nextVersion:number;now:number;})=>db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'legal',?,?,'Legal approval needs your decision',?)").bind(facts.identifier,facts.companyId,facts.approver_user_id,facts.itemId,facts.nextVersion,facts.now)
};}
