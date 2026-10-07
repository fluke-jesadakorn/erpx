import {z} from 'zod';
export const publicMarketingSubmissionSchema=z.object({contactName:z.string().trim().min(2).max(160),contactDetail:z.string().trim().min(3).max(2000),brief:z.string().trim().min(3).max(3000),consentAccepted:z.literal(true),source:z.string().trim().min(2).max(120),campaignId:z.string().min(1).optional()});
export type PublicMarketingSubmission=z.infer<typeof publicMarketingSubmissionSchema>;
export type PublicMarketingReceipt={id:string;status:'pending'};
