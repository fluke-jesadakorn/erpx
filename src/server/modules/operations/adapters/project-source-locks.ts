import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Project structure can inspect/link original Tasks. Match the existing Task
 * owner order: acquire its writer source before a Project writer source, never
 * after a Company advisory lock or an already established data snapshot. */
export function projectTaskSourceReadLocks(database:SqlDatabase){
 return database.prepare('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
}
