import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import type {StaffActor} from '@erpx/operations-server/domain/staff-policy';
import {routineSourceReadScope} from '@erpx/operations-server/adapters/routine-source-read';
import {OPERATIONS_SELECT_1_FROM} from './read-tail-r56-sources';

export function workRoutineReadScope(database:SqlDatabase,actor:StaffActor){return routineSourceReadScope(database,actor);}
export async function requireWorkRoutineRead(database:SqlDatabase,actor:StaffActor,id:string){const scope=await workRoutineReadScope(database,actor);if(!await database.prepare(OPERATIONS_SELECT_1_FROM(`routine_templates t WHERE t.id=? AND t.company_id=? AND ${scope.where}`)).bind(id,actor.companyId,...scope.args).first())throw new ApiError(404,'TEMPLATE_NOT_FOUND','Routine template not found.');}
