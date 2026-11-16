import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {learningAssignmentNoticeSource} from "@erpx/server-learning/public/notification-source";
export function learningAssignmentNotice(db:SqlDatabase,change:"assigned"|"changed",noticeId:string,id:string){return db.prepare("SELECT 1").bind();}
