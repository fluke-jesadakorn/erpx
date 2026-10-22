import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";
import type {Clock} from '@erpx/server-shared/index';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {priceListSchema,priceListUpdateSchema,priceListDecisionSchema,validatePriceList,type PriceListInput,type PriceListUpdate,type PriceListDecision,type PriceListRow} from '@erpx/server-sales/domain/price-lists';
export type PriceListMutation={key:string;id:string;operation:string;now:number;result:Record<string,unknown>} & ({action:'create';input:PriceListInput;products:PriceListRow[];prices:number[]} | {action:'update';input:PriceListUpdate;products:PriceListRow[];prices:number[]} | {action:'decide';input:PriceListDecision});
export interface PriceListRepository {
 workspace():Promise<{lists:PriceListRow[];items:PriceListRow[];products:PriceListRow[];customers:PriceListRow[];settings:PriceListRow|null}>;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 list(id:string):Promise<PriceListRow|null>;currency():Promise<PriceListRow|null>;customer(id:string,currency:string):Promise<boolean>;product(id:string):Promise<PriceListRow|null>;
 commit(mutation:PriceListMutation):Promise<Record<string,unknown>>;
}
export function createPriceListApplication(ports:{authorize():void;authorizeReview():void;companyScope:boolean;canApprove:boolean;isSamePerson(id:string|number|null):boolean;clock:Clock;repository():Promise<PriceListRepository>}){
 function authorize(){ports.authorize();if(!ports.companyScope)throw new BusinessRuleError(403,'COMPANY_PRICING_SCOPE_REQUIRED','Use a Company sales assignment to manage price lists.');}
 async function sources(repository:PriceListRepository,input:PriceListInput){
  const settings=await repository.currency();if(!settings||settings.currency!==input.currency)throw new BusinessRuleError(422,'PRICE_LIST_CURRENCY_INVALID','Use the company operating currency.');
  if(input.customerId&&!await repository.customer(input.customerId,input.currency))throw new BusinessRuleError(422,'PRICE_LIST_CUSTOMER_INVALID','Choose an active customer in this currency.');
  const products:PriceListRow[]=[];for(const item of input.items){const product=await repository.product(item.productId);if(!product)throw new BusinessRuleError(422,'PRICE_LIST_PRODUCT_INVALID','Every price-list product must be active.');products.push(product);}return products;
 }
 return {
  async workspace(){authorize();const facts=await (await ports.repository()).workspace();return {lists:facts.lists,items:facts.items,products:facts.products,customers:facts.customers,currency:String(facts.settings?.currency??''),canApprove:ports.canApprove};},
  async create(raw:unknown,key:string){const input=priceListSchema.parse(raw);authorize();validatePriceList(input);const repository=await ports.repository(),id=`price-list-${key}`,operation='sales.price-list.create',prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const products=await sources(repository,input),prices=input.items.map(item=>decimalToMinor(item.price)),now=ports.clock.now(),result={id,status:'draft',version:1};return repository.commit({action:'create',key,id,operation,input,products,prices,now,result});
  },
  async update(id:string,raw:unknown,key:string){const input=priceListUpdateSchema.parse(raw);authorize();validatePriceList(input);const repository=await ports.repository(),operation='sales.price-list.update',prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const list=await repository.list(id);if(!list)throw new BusinessRuleError(404,'PRICE_LIST_NOT_FOUND','Price list not found.');if(list.status!=='draft')throw new BusinessRuleError(409,'PRICE_LIST_LOCKED','Only a draft price list can be edited.');
   const products=await sources(repository,input),prices=input.items.map(item=>decimalToMinor(item.price)),now=ports.clock.now(),result={id,status:'draft',version:input.expectedVersion+1};return repository.commit({action:'update',key,id,operation,input,products,prices,now,result});
  },
  async decide(id:string,raw:unknown,key:string){const input=priceListDecisionSchema.parse(raw);authorize();ports.authorizeReview();const repository=await ports.repository(),operation=`sales.price-list.${input.action}`,prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   const list=await repository.list(id);if(!list)throw new BusinessRuleError(404,'PRICE_LIST_NOT_FOUND','Price list not found.');if(list.status!=='draft')throw new BusinessRuleError(409,'PRICE_LIST_LOCKED','Only a draft price list can be decided.');if(input.action==='activate'&&ports.isSamePerson(list.prepared_by??list.created_by))throw new BusinessRuleError(403,'INDEPENDENT_PRICING_APPROVER_REQUIRED','Another sales manager must activate this price list.');
   const now=ports.clock.now(),result={id,status:input.action==='activate'?'active':'cancelled',version:input.expectedVersion+1};return repository.commit({action:'decide',key,id,operation,input,now,result});
  }
 };
}
