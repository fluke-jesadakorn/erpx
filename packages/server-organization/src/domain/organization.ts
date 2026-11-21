import {reservedWorkspaceSlug} from '@erpx/contracts/holding-hosts';
import {z} from 'zod';
import {BusinessRuleError} from '@erpx/server-shared/index';
export const positionSchema = z.object({
  requirements: z.string().trim().max(3000).default(""),
  title: z.string().trim().min(2).max(160), branchId: z.string().nullable(), departmentId: z.string().nullable(),
  levelId: z.string().min(1), reportsToPositionId: z.string().nullable(), isCeo: z.boolean().default(false),
  rosterRequired: z.boolean().optional(),
  active: z.boolean().default(true), expectedVersion: z.number().int().min(0), reason: z.string().trim().min(3).max(1000),
});
export const organizationNodeChangeSchema=z.object({
  action:z.enum(["create","rename","suspend","reactivate","archive","relocate"]),targetNodeId:z.string().min(1).nullable().default(null),
  kind:z.enum(["branch","department"]),slug:z.string().trim().regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/).max(80).nullable().default(null),
  displayName:z.string().trim().min(2).max(120),branchId:z.string().min(1).nullable().default(null),
  expectedNodeVersion:z.number().int().positive().nullable().default(null),expectedVersion:z.number().int().min(0),reason:z.string().trim().min(3).max(1000),
  previewSnapshot:z.string().regex(/^[a-f0-9]{64}$/).optional(),
}).superRefine((input,context)=>{if(input.action==='create'&&input.slug&&reservedWorkspaceSlug(input.slug))context.addIssue({code:'custom',path:['slug'],message:'Choose a workspace slug distinct from applications and system routes.'});});
export const organizationNodeDecisionSchema=z.object({expectedVersion:z.number().int().positive(),decision:z.enum(["approve","reject"]),reason:z.string().trim().min(3).max(1000),attachmentSetIds:z.array(z.string().min(1)).max(20).optional()});
export const organizationChangeFiltersSchema=z.object({
  changeId:z.string().trim().min(1).max(200).nullable().catch(null),
  changeState:z.enum(["open","all","draft","submitted","rejected","applied","cancelled"]).catch("open"),
  changeQuery:z.string().trim().max(120).catch(""),
  changePage:z.coerce.number().int().positive().max(100000).catch(1),
});

export function checkScope(actor: {branchId:string|null;departmentId:string|null}, branchId:string|null, departmentId:string|null) {
  if ((actor.branchId && actor.branchId!==branchId) || (actor.departmentId && actor.departmentId!==departmentId)) throw new BusinessRuleError(403,"SCOPE_FORBIDDEN","Choose a position in your authorized workspace.");
}

export type NodeChangeInput=z.infer<typeof organizationNodeChangeSchema>;export type NodeDecisionInput=z.infer<typeof organizationNodeDecisionSchema>;export type ChangeFilters=z.infer<typeof organizationChangeFiltersSchema>;
