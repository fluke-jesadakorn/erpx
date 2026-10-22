import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function operationsRoutineJournalEffects(database:SqlDatabase){return {stageGenerateOccurrencesEffect2(key: string, key2: string, value: "system", value2: "scheduler", value3: "routine.occurrence", value4: "staff_task", id: string, value5: 1, value6: "Generate scheduled occurrence", now: number) {
            return database.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)")
                .bind(key, key2, value, value2, value3, value4, id, value5, value6, now);
        },
stageGenerateOccurrencesEffect3(key: string, key2: string, value: "task.generated", value2: "staff_task", id: string, value3: 1, value4: string, now: number) {
            return database.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(key, key2, value, value2, id, value3, value4, now);
        }};}
