import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function peopleLifecycleNotificationEffects(database:SqlDatabase){return {stageSaveLifecycleEffect11(value: string, companyId: string, owner: string, taskId: string, value2: string, now: number) { return database.prepare("SELECT 1").bind(); }};}
