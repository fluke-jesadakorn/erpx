import {z} from "zod";
import {BusinessRuleError} from "@erpx/server-shared/index";
const references=z.object({contentAssetId:z.string().min(1).max(180).optional(),contentVersion:z.number().int().positive().optional(),mediaPublicationId:z.string().min(1).max(180).optional()}).superRefine((value,context)=>{
 if(Boolean(value.contentAssetId)!==Boolean(value.contentVersion))context.addIssue({code:'custom',message:'Choose an exact content revision.'});
 if(value.mediaPublicationId&&!value.contentAssetId)context.addIssue({code:'custom',message:'Choose the content owner of the public creative.'});
});
export function marketingPresentationReferences(configuration:unknown){
 let value=configuration;if(typeof value==='string'){try{value=JSON.parse(value);}catch{throw new BusinessRuleError(422,'MARKETING_PRESENTATION_INVALID','Review the saved form presentation.');}}
 const parsed=references.safeParse(value);if(!parsed.success)throw new BusinessRuleError(422,'MARKETING_PRESENTATION_INVALID',parsed.error.issues[0]?.message??'Review the saved form presentation.');return parsed.data;
}
export type MarketingPresentationReferences=z.infer<typeof references>;
