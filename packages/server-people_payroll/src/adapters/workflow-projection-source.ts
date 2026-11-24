import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { ppRd27StateVersionScopeByIdSql } from './pp-read-tail-r27-sources';
import type {ProjectionSource} from '@erpx/server-workflow/public/job-projector-contract';
export function peopleProjectionSource(db:SqlDatabase,kind:'overtime'|'leave'|'payroll',id:string){return db.prepare(kind==='overtime'?ppRd27StateVersionScopeByIdSql('overtime_requests'):kind==='leave'?ppRd27StateVersionScopeByIdSql('leave_requests'):'SELECT status state,version,company_id,NULL branch_id,NULL department_id FROM hr_payroll_runs WHERE id=?').bind(id).first<ProjectionSource>();}
