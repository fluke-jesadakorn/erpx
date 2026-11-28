import "server-only";
import {PP_RD18_HR_PEOPLE_PROJ_SQL} from './pp-read-tail-r18-sources';
/** Closed native projections for Inventory, Planning, Manufacturing and Project snapshots. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const inventoryExecutiveOwnerProjections = Object.freeze({
  hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL,
} as const);
