import {z} from 'zod';
import {assetSchema,assetEditSchema,assetActivationSchema,assetDecisionSchema,assetDisposalSchema} from './assets';
export const assetsFinancialCreateSchema=assetSchema.strict();
export const assetsFinancialEditSchema=assetEditSchema.strict();
export const assetsFinancialRecognitionSchema=assetActivationSchema.strict();
export const assetsFinancialDecisionSchema=assetDecisionSchema.strict();
export const assetsFinancialDisposalSchema=assetDisposalSchema.strict();
export type AssetsFinancialCreateInput=z.infer<typeof assetsFinancialCreateSchema>;
export type AssetsFinancialEditInput=z.infer<typeof assetsFinancialEditSchema>;
export type AssetsFinancialRecognitionInput=z.infer<typeof assetsFinancialRecognitionSchema>;
export type AssetsFinancialDecisionInput=z.infer<typeof assetsFinancialDecisionSchema>;
export type AssetsFinancialDisposalInput=z.infer<typeof assetsFinancialDisposalSchema>;
export type AssetsFinancialAssetSummary={id:string;assetNumber:string;name:string;status:string;version:number;currency:string;costMinor:number;bookValueMinor:number};
export type AssetsFinancialAsset=AssetsFinancialAssetSummary&{chartId:string;accountId:string;depreciationAccountId:string|null;depreciationExpenseAccountId:string|null;residualMinor:number;usefulLifeMonths:number;inServiceDate:string;accumulatedDepreciationMinor:number;activationJournalId:string|null;canEdit:boolean;canCancel:boolean;canRecognize:boolean;canPrepareDisposal:boolean};
export type AssetsFinancialDisposal={id:string;assetId:string;status:string;version:number;assetVersion:number;entryDate:string;proceedsMinor:number;journalId:string|null;canPost:boolean;canCancel:boolean};
export type AssetsFinancialWorkspace={
 companyId:string;available:boolean;contextKey:string|null;canCreate:boolean;canReadPhysicalRegister:boolean;canRegisterPhysical:boolean;assets:AssetsFinancialAssetSummary[];
 detail:null|{asset:AssetsFinancialAsset;disposals:AssetsFinancialDisposal[];events:{id:string;action:string;assetVersion:number;createdAt:number}[];depreciations:{id:string;month:string;entryDate:string;amountMinor:number;journalId:string}[];physicalLinks:{id:string;code:string;name:string;status:string;version:number}[]};
 accounts:{id:string;chartId:string;code:string;name:string;category:string;currency:string|null}[];
 periods:{id:string;name:string;startDate:string;endDate:string;state:string}[];
 dimensions:{id:string;code:string;name:string;required:boolean}[];dimensionValues:{dimensionId:string;code:string;name:string}[];
 unavailableReason:'authority-required'|'purchase-required'|null;
};
export type AssetsFinancialResult={id:string;status:string;version:number;cost?:string;cancelled?:boolean;journalId?:string|null;assetId?:string;assetVersion?:number;secondaryJournalIds?:string[]};

/** Exact fields emitted by the existing Finance asset applications. */
export const assetsFinancialResultSchema=z.object({id:z.string().min(1),status:z.string().min(1),version:z.number().int().positive(),cost:z.string().optional(),cancelled:z.boolean().optional(),journalId:z.string().nullable().optional(),assetId:z.string().optional(),assetVersion:z.number().int().positive().optional(),secondaryJournalIds:z.array(z.string()).optional()}).strict();
