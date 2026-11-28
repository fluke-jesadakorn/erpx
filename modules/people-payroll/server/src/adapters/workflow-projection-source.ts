import {ppRd36SinglePred} from './pp-read-tail-r36-sources';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import { ppRd27StateVersionScopeByIdSql } from './pp-read-tail-r27-sources';
import type {ProjectionSource} from '@erpx/workflow-server/public/job-projector-contract';
export function peopleProjectionSource(db:SqlDatabase,kind:'overtime'|'leave'|'payroll',id:string){return db.prepare(kind==='overtime'?ppRd27StateVersionScopeByIdSql('overtime_requests'):kind==='leave'?ppRd27StateVersionScopeByIdSql('leave_requests'):ppRd36SinglePred("status state,version,company_id,NULL branch_id,NULL department_id", "hr_payroll_runs", "id")).bind(id).first<ProjectionSource>();}
