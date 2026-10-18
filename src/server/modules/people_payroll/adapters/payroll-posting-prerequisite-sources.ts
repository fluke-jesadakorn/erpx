import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';

type Group={id:string;name:string;currency:string;status:'draft'|'active'|'archived';source:string};
/** Named Finance setup composes these original Payroll facts in its actual
 * Holding unit. The full rows remain private to source/finality comparisons. */
export function payrollPostingPrerequisiteSourceLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE hr_pay_groups,hr_payroll_account_mappings IN SHARE MODE')];
}
export async function payrollPostingPrerequisiteSources(database:SqlDatabase,companyId:string){
 const [groups,mappings]=await Promise.all([
  database.prepare('SELECT g.id,g.name,g.currency,g.status,to_jsonb(g)::text source FROM hr_pay_groups g WHERE g.company_id=? ORDER BY g.id').bind(companyId).all<Group>(),
  database.prepare('SELECT m.id,to_jsonb(m)::text source FROM hr_payroll_account_mappings m WHERE m.company_id=? ORDER BY m.id').bind(companyId).all<{id:string;source:string}>(),
 ]);
 return {groups:groups.results,image:JSON.stringify([groups.results.map(row=>row.source),mappings.results.map(row=>row.source)])};
}
