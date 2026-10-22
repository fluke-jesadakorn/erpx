import {z} from "zod";
export const marketingCampaignCommand=z.discriminatedUnion('action',[
 z.object({action:z.literal('create'),code:z.string().trim().min(2).max(60).regex(/^[A-Za-z0-9_-]+$/),name:z.string().trim().min(2).max(160)}),
 z.object({action:z.literal('link-cost'),campaignId:z.string().min(1),journalLineId:z.string().min(1),evidence:z.string().trim().min(3).max(2000)})
]);
export type MarketingCostSource = {
 source_line_id:string;journal_number:string;entry_date:string;currency:string;
 account_code:string;account_name:string;description:string;debit_minor:string;
};
export type MarketingCampaignInput=z.infer<typeof marketingCampaignCommand>;
