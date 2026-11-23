import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Single fixed-SQL read of Project member ids, shared by the Work UI, People entrypoint and purchase command consumers. */
export function projectMemberRows(db:SqlDatabase,projectId:string){return db
        .prepare("SELECT user_id FROM staff_project_members WHERE project_id=? ORDER BY user_id")
        .bind(projectId)
        .all<{user_id:string}>();}
export function workUiStaffFacts(db:SqlDatabase){return {
async projectMembers(value:string){return projectMemberRows(db,value);}
};}
