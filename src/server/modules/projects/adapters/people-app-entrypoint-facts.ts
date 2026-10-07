import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function projectMemberIds(db:SqlDatabase,projectId:string){return db
        .prepare("SELECT user_id FROM staff_project_members WHERE project_id=?")
        .bind(projectId)
        .all<{ user_id: string }>();}
