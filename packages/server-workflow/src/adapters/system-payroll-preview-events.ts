import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function systemPayrollPreviewEvents(database: SqlDatabase) {
    return { stageDrainPayrollScheduledPreviewsCompleted3(value: string, commandId: string, id: string, value2: number, timestamp: number) {
            return database.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)
          VALUES(?,?,'system:payroll-schedule','system:payroll-schedule','people.payroll.calculate','people_payroll',?,?,'Scheduled gross-pay preview',?)`)
                .bind(value, commandId, id, value2, timestamp).run();
        },
        stageDrainPayrollScheduledPreviewsCompleted4(value: string, commandId: string, id: string, value2: number, value3: string, timestamp: number) {
            return database.prepare(`INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)
          VALUES(?,?,'people.payroll.calculate','people_payroll',?,?,?,?)`)
                .bind(value, commandId, id, value2, value3, timestamp).run();
        } };
}
