import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function socialReplyEngineeringLock(db:SqlDatabase,company:string){return db.prepare("SELECT 1").bind();}
