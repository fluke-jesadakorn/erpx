import {projectPointReadHead} from './read-tail-r33';
/** Byte-identical point read shared by the Project task product-entry and
 * handoff project lookups. The emitted SQL is the r33 point-read head with the
 * closed `project_id` projection and `company_id` second key; only the source
 * table varies, and bind order stays exact. No SQL comes from a caller. */
export const projectIdSql=(table:string)=>projectPointReadHead('project_id',table,'company_id');
