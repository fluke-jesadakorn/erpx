import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function peopleLifecycleAuditEffects(database:SqlDatabase){return {stageSaveLifecycleEffect5(value: string, key: string, userId: string, assignmentId: string, value2: "people.lifecycle.supersede", id: string, value3: number, value4: string, now: number, key2: string) {
            return database.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,'people_lifecycle',?,?,?,?,(SELECT authority_json FROM command_receipts WHERE command_id=?))`)
                .bind(value, key, userId, assignmentId, value2, id, value3, value4, now, key2);
        }};}
