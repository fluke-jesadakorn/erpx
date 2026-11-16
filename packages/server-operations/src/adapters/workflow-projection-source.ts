import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ProjectionSource} from '@erpx/server-workflow/public/job-projector-contract';
export function taskProjectionSource(db:SqlDatabase,id:string){return db.prepare("SELECT 1").bind().first<ProjectionSource>();}
