import {z} from "zod";
import {productionScope,legalProductionScope,allReleaseCriteria,criteriaForScope,type ReleaseScopeVersion,type GateState,type ReleaseCriterion} from "@erpx/catalogue/production-scope";
const reason = z.string().trim().min(3).max(2000);
const criterionId = z.enum(allReleaseCriteria.map(row => row.id) as [ReleaseCriterion, ...ReleaseCriterion[]]);
export const scopeVersionSchema = z.enum([productionScope.version, legalProductionScope.version]);
export const evidenceSchema = z.object({
  criterionId,
  summary: reason,
  files: z.array(z.object({
    reference: z.string().trim().min(1).max(1000),
    sha256: z.string().regex(/^[a-f0-9]{64}$/),
  }).strict()).min(1).max(30),
}).strict();
export const releaseSubmitSchema = z.object({
  scopeVersion: scopeVersionSchema,
  softwareVersion: z.string().min(1).max(100),
  expectedConfigurationRevision: z.number().int().min(0),
  expectedSetupVersion: z.number().int().positive(),
  reason,
  evidence: z.array(evidenceSchema).min(1).max(allReleaseCriteria.length),
}).strict().superRefine((input, ctx) => {
  const required = criteriaForScope(input.scopeVersion);
  if (input.evidence.length !== required.length || new Set(input.evidence.map(row => row.criterionId)).size !== required.length ||
      input.evidence.some(row => !required.some(criterion => criterion.id === row.criterionId)))
    ctx.addIssue({ code: 'custom', message: 'Supply evidence for every release criterion exactly once.' });
});
export const releaseReviewSchema = z.object({
  expectedVersion: z.number().int().positive(),
  criterionId,
  state: z.enum(['pending', 'passed']),
  reason,
}).strict();
export const releaseRevokeSchema = z.object({ expectedVersion: z.number().int().positive(), reason }).strict();

export type ReleaseRow = {
  id: string; company_id: string; scope_version: string; software_version: string;
  configuration_revision: number; setup_version: number; evidence_sha256: string;
  evidence_json: string; requested_by: string; version: number; revoked_at: number | null;
  reason: string; created_at: number; updated_at: number;
};
export type ReviewRow = { criterion_id: string; state: GateState; reviewer_id: string; reason: string; reviewed_at: number; authority_current: boolean };

export type ReleaseInputs = {scopeVersion:ReleaseScopeVersion;softwareVersion:string;configurationRevision:number;setupVersion:number;setupAccepted:boolean;establishmentCount:number;currencyCompatible:boolean};
export type ReleaseSubmit = z.infer<typeof releaseSubmitSchema>;
export type ReleaseReview = z.infer<typeof releaseReviewSchema>;
export type ReleaseRevoke = z.infer<typeof releaseRevokeSchema>;
export function inputBlockers(row: ReleaseRow, inputs: ReleaseInputs) {
  return [
    ...(row.revoked_at ? ['Release acceptance was revoked.'] : []),
    ...(inputs.softwareVersion === 'unbuilt' ? ['Use an isolated production release build for release acceptance.'] : []),
    ...(row.scope_version !== inputs.scopeVersion ? ['The release scope changed.'] : []),
    ...(row.software_version !== inputs.softwareVersion ? ['The software build changed.'] : []),
    ...(Number(row.configuration_revision) !== inputs.configurationRevision ? ['Material Company configuration changed.'] : []),
    ...(!inputs.setupAccepted || Number(row.setup_version) !== inputs.setupVersion ? ['Company setup needs current independent acceptance.'] : []),
    ...(inputs.scopeVersion === productionScope.version && inputs.establishmentCount !== 1 ? ['This release requires exactly one active establishment.'] : []),
    ...(!inputs.currencyCompatible ? ['Company settings must use THB and Thai payroll jurisdiction.'] : []),
  ];
}

