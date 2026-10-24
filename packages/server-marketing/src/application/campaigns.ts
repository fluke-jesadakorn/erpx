import type {Clock} from "@erpx/server-shared/index";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {marketingCampaignCommand,type MarketingCampaignInput,type MarketingCostSource} from "@erpx/server-marketing/domain/campaigns";
export type MarketingCampaignMutation={input:MarketingCampaignInput;id:string;key:string;operation:string;now:number};
export function createMarketingCampaignApplication(ports:{clock:Clock;authorize(write?:boolean):void;authorizeFinance():void;campaigns():Promise<Row[]>;costSources():Promise<MarketingCostSource[]>;replay<Result=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(mutation:MarketingCampaignMutation,result:Result):Promise<Result>}){
 return {
  async campaigns(){ports.authorize();return ports.campaigns();},
  async costSources(){ports.authorize();ports.authorizeFinance();return ports.costSources();},
  async change(raw:unknown,key:string){ports.authorize(true);const input=marketingCampaignCommand.parse(raw);if(input.action==="link-cost")ports.authorizeFinance();
   const id=input.action==="create" ? `campaign-${key}` : `campaign-cost-${key}`,operation=`marketing.campaign.${input.action}`,prior=await ports.replay<{id:string;version:number}>(key,operation,id,input);if(prior)return prior;
   return ports.commit({input,id,key,operation,now:ports.clock.now()},{id,version:1});
  },
 };
}
