import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {learningAssignmentNoticeSource} from "@erpx/server-learning/public/notification-source";
export function learningAssignmentNotice(db:SqlDatabase,change:"assigned"|"changed",noticeId:string,id:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) ${learningAssignmentNoticeSource(change)}`).bind(noticeId,id);}
