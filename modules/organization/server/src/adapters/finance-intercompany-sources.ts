import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {activeNodeByKind} from './organization-column-alias-families-r27';
export function financeIntercompanyActiveCompany(db:SqlDatabase,id:string){return db.prepare(activeNodeByKind('id','company')).bind(id).first<{id:string}>();}
/** Organization owns this active-company source; Finance adds its own entity exclusion in the same supplied query. */
export const financeIntercompanyAvailableCompanySource="SELECT n.id AS company_id,n.display_name FROM organization_nodes n WHERE n.kind='company' AND n.status='active'";
