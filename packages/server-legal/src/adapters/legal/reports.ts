import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {featureSpecs} from "@erpx/feature-legal/lib/legal/features";
import {createLegalReportsQuery} from "@erpx/server-legal/application/reports";
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
/** Exact projection of the native `reports` auxiliary read; the transport accepts
 * every authoritative field and no SQL text remains in TypeScript. */
type LegalReportsRead = {
  asOf: number;
  registers: Record<string,unknown>[];
  upcoming: Record<string,unknown>[];
  workflows: {category:string;rows:Record<string,unknown>[]}[];
  costs: Record<string,unknown>[];
  access: Record<string,unknown>[];
  jobs: Record<string,unknown>[];
  invitations: Record<string,unknown>[];
};
export async function legalReports(_db: SqlDatabase, actor: WorkContext){
  // Preserve the original deny-first precedence: authorization is decided
  // before transport configuration or any native read is consulted.
  requirePermission(actor,"legal.read");
  const read=await nativeLegalAuxiliaryRead<LegalReportsRead>(actor,'reports');
  return createLegalReportsQuery({require:()=>requirePermission(actor,"legal.read"),
    clock:{now:()=>read.asOf},featureCategories:Object.keys(featureSpecs),
    repository:{
      registers:async()=>({results:read.registers}),
      upcoming:async()=>({results:read.upcoming}),
      workflow:async category=>({results:read.workflows.find(row=>row.category===category)?.rows??[]}),
      costs:async()=>({results:read.costs}),
      access:async()=>({results:read.access}),
      jobs:async()=>({results:read.jobs}),
      invitations:async()=>({results:read.invitations}),
    }})();
}
