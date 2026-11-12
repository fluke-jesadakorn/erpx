import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {StaffActor} from '@erpx/server-operations/domain/staff-policy';
import {routineSourceReadScope} from '@erpx/server-operations/adapters/routine-source-read';

export function workRoutineReadScope(database:SqlDatabase,actor:StaffActor){return routineSourceReadScope(database,actor);}
export async function requireWorkRoutineRead(database:SqlDatabase,actor:StaffActor,id:string){const scope=await workRoutineReadScope(database,actor);if(!await database.prepare(`SELECT 1 FROM routine_templates t WHERE t.id=? AND t.company_id=? AND ${scope.where}`).bind(id,actor.companyId,...scope.args).first())throw new ApiError(404,'TEMPLATE_NOT_FOUND','Routine template not found.');}
