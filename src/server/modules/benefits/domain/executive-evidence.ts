import {z} from "zod";
import {filtersSchema} from "../../runtime/public/rules/canvas-domain";
import {BusinessRuleError} from "../../shared";
export const benefitEvidenceMetrics=["active-benefit-elections","published-benefit-plans"] as const;
export type BenefitEvidenceMetric=typeof benefitEvidenceMetrics[number];
export const benefitEvidenceProvider="benefits-current-records";
export const benefitEvidenceFields=["id","status","version","updated_at","effective_from","effective_to"];
export const benefitEvidenceInput=z.object({metric:z.enum(benefitEvidenceMetrics),filters:filtersSchema,query:z.string().trim().max(100).default(""),cursor:z.string().max(2000).optional(),limit:z.coerce.number().int().min(1).max(50).default(25)}).strict();
export type BenefitEvidenceInput=z.infer<typeof benefitEvidenceInput>;
export type BenefitEvidenceRow={id:string;status:string;version:number;updated_at:number;effective_from:string;effective_to:string|null;label:string};
export function benefitEvidenceCursor(value:string|undefined){if(!value)return null;try{return z.object({id:z.string().min(1).max(200)}).strict().parse(JSON.parse(Buffer.from(value,"base64url").toString("utf8")));}catch{throw new BusinessRuleError(422,"EVIDENCE_CURSOR_INVALID","The Benefits evidence cursor is invalid. Refresh this list.");}}
export function benefitEvidenceVersion(value:number){if(!Number.isSafeInteger(Number(value))||Number(value)<1)throw new BusinessRuleError(500,"EVIDENCE_VALUE_INVALID","The original Benefits revision is invalid.");return Number(value);}
