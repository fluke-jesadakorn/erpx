import {z} from 'zod';
import {BusinessRuleError} from '@erpx/server-shared/index';
import type {OrganizationTemplatePosition} from "@erpx/server-organization/public/rules/organization-domain";
export const positionSchema = z.object({
  requirements: z.string().trim().max(3000).default(""),
  title: z.string().trim().min(2).max(160), branchId: z.string().nullable(), departmentId: z.string().nullable(),
  levelId: z.string().min(1), reportsToPositionId: z.string().nullable(), isCeo: z.boolean().default(false),
  rosterRequired: z.boolean().optional(),
  active: z.boolean().default(true), expectedVersion: z.number().int().min(0), reason: z.string().trim().min(3).max(1000),
});
export const templatePositionSchema = z.object({key:z.string().min(1).max(100),title:z.string().trim().min(2).max(160),levelNumber:z.number().int().min(1).max(6),reportsToKey:z.string().nullable(),isCeo:z.boolean(),rosterRequired:z.boolean().default(false)});
export const organizationTemplateSchema = z.object({name:z.string().trim().min(2).max(120),positions:z.array(templatePositionSchema).min(1).max(100),expectedVersion:z.number().int().min(0)});
export const applyOrganizationTemplateSchema = z.object({expectedVersion:z.number().int().positive(),branchId:z.string().nullable(),departmentId:z.string().nullable(),reason:z.string().trim().min(3).max(1000)});
export const organizationNodeChangeSchema=z.object({
  action:z.enum(["create","rename","suspend","reactivate","archive","relocate"]),targetNodeId:z.string().min(1).nullable().default(null),
  kind:z.enum(["branch","department"]),slug:z.string().trim().regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/).max(80).nullable().default(null),
  displayName:z.string().trim().min(2).max(120),branchId:z.string().min(1).nullable().default(null),
  expectedNodeVersion:z.number().int().positive().nullable().default(null),expectedVersion:z.number().int().min(0),reason:z.string().trim().min(3).max(1000),
  previewSnapshot:z.string().regex(/^[a-f0-9]{64}$/).optional(),
});
export const organizationNodeDecisionSchema=z.object({expectedVersion:z.number().int().positive(),decision:z.enum(["approve","reject"]),reason:z.string().trim().min(3).max(1000),attachmentSetIds:z.array(z.string().min(1)).max(20).optional()});
export const organizationChangeFiltersSchema=z.object({
  changeId:z.string().trim().min(1).max(200).nullable().catch(null),
  changeState:z.enum(["open","all","draft","submitted","rejected","applied","cancelled"]).catch("open"),
  changeQuery:z.string().trim().max(120).catch(""),
  changePage:z.coerce.number().int().positive().max(100000).catch(1),
});

export function validateTemplate(positions:OrganizationTemplatePosition[]) {
  if(new Set(positions.map(p=>p.key)).size!==positions.length)throw new BusinessRuleError(422,"DUPLICATE_POSITION","Template position keys must be unique.");
  for(const p of positions){if(p.isCeo&&(p.levelNumber!==1||p.reportsToKey))throw new BusinessRuleError(422,"CEO_POSITION_INVALID","The CEO uses Level 1 with no reporting position.");if(!p.isCeo&&!p.reportsToKey)throw new BusinessRuleError(422,"REPORTING_POSITION_REQUIRED","Each non-CEO position needs a reporting position.");const seen=new Set([p.key]);let parent=p.reportsToKey;while(parent){if(seen.has(parent))throw new BusinessRuleError(422,"REPORTING_CYCLE","Correct the template reporting cycle.");seen.add(parent);const next=positions.find(v=>v.key===parent);if(!next||next.levelNumber>=p.levelNumber)throw new BusinessRuleError(422,"REPORTING_POSITION_INVALID","Template managers must exist and be more senior.");parent=next.reportsToKey;}}
  if(positions.filter(p=>p.isCeo).length!==1)throw new BusinessRuleError(422,"CEO_POSITION_REQUIRED","The organization template needs one CEO position.");
}
export function checkScope(actor: {branchId:string|null;departmentId:string|null}, branchId:string|null, departmentId:string|null) {
  if ((actor.branchId && actor.branchId!==branchId) || (actor.departmentId && actor.departmentId!==departmentId)) throw new BusinessRuleError(403,"SCOPE_FORBIDDEN","Choose a position in your authorized workspace.");
}

export type PositionInput=z.infer<typeof positionSchema>;export type TemplateInput=z.infer<typeof organizationTemplateSchema>;export type ApplyTemplateInput=z.infer<typeof applyOrganizationTemplateSchema>;export type NodeChangeInput=z.infer<typeof organizationNodeChangeSchema>;export type NodeDecisionInput=z.infer<typeof organizationNodeDecisionSchema>;export type ChangeFilters=z.infer<typeof organizationChangeFiltersSchema>;
