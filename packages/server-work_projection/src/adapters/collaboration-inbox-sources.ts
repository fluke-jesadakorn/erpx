import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {appointmentProbe,invitedAttendeeSql} from './read-tail-sql';
export function collaborationInboxSources(db:SqlDatabase){return {async calendarAppointment(source_id:string,companyId:string){return db.prepare(appointmentProbe('id,version'))
      .bind(source_id,companyId).first<{id:string;version:number}>();},async calendarRecipient(source_id:string,companyId:string,userId:string,source_id2:string,companyId2:string,userId2:string){return db.prepare(`${invitedAttendeeSql}
        UNION ALL ${appointmentProbe('1',' AND created_by=?')} LIMIT 1`)
        .bind(source_id,companyId,userId,source_id2,companyId2,userId2).first();}};}
