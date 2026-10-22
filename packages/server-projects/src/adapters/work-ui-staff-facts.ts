import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workUiStaffFacts(db:SqlDatabase){return {
async projectMembers(value:string){return db
                    .prepare(
                      "SELECT user_id FROM staff_project_members WHERE project_id=?",
                    )
                    .bind(value)
                    .all();}
};}
