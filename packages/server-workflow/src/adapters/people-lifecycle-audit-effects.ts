import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
export function peopleLifecycleAuditEffects(database:SqlDatabase){return {stageSaveLifecycleEffect5(value: string, key: string, userId: string, assignmentId: string, value2: "people.lifecycle.supersede", id: string, value3: number, value4: string, now: number, key2: string) {
            return nativeWorkflowWrite('wf_native_people_lifecycle_audit_effects_19', [value, key, userId, assignmentId, value2, id, value3, value4, now, key2]);
        }};}
