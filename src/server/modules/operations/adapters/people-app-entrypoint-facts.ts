import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {ScheduleRow} from '../domain/routines';
export function routineTemplateSchedules(db:SqlDatabase,templateId:string,companyId:string){return db
        .prepare(
          "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at",
        )
        .bind(templateId, companyId)
        .all<ScheduleRow>();}
