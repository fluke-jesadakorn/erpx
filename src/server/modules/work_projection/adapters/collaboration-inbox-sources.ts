import type {SqlDatabase} from '../../runtime/public/runtime/types';export function collaborationInboxSources(db:SqlDatabase){return {async calendarAppointment(source_id:string,companyId:string){return db.prepare("SELECT id,version FROM calendar_appointments WHERE id=? AND company_id=?")
      .bind(source_id,companyId).first<{id:string;version:number}>();},async calendarRecipient(source_id:string,companyId:string,userId:string,source_id2:string,companyId2:string,userId2:string){return db.prepare(`SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?
        UNION ALL SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=? AND created_by=? LIMIT 1`)
        .bind(source_id,companyId,userId,source_id2,companyId2,userId2).first();}};}
