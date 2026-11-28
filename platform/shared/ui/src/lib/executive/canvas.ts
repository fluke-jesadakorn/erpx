export * from "@erpx/platform-runtime-server/public/rules/canvas-domain";
import {defaultFilters as explicitDefaultFilters,type Filters} from "@erpx/platform-runtime-server/public/rules/canvas-domain";
/** UI/native compatibility: applications use the explicit-date domain helper. */
export function defaultFilters(now = new Date()):Filters {return explicitDefaultFilters(now);}
