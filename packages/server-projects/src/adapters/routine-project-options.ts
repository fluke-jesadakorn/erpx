import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {optionalProjectOptions} from '@erpx/server-projects/adapters/optional-project-options';

/** Projects is optional when editing standalone Operations routines. A missing
 * Projects purchase removes only its picker choices; a real Project choice
 * retains the original current assignment, record policy and purchase reader.
 * Console disabled/shadow behavior remains compatible; configured Work hosts
 * can request strict purchased references without changing the global mode. */
export async function routineProjectOptions(database:SqlDatabase,actor:StaffActor,options?:{strictProduct?:boolean}){
 return optionalProjectOptions(database,actor,options);
}
