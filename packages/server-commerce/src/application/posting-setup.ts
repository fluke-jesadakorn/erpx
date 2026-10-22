import {BusinessRuleError} from '@erpx/server-shared/index';
import {commercePostingSetupSchema,type CommercePostingSetupInput,type CommercePostingSetupConfiguration,type CommercePostingSetupResult} from '@erpx/server-commerce/domain/posting-setup';

export interface CommercePostingSetupPorts {
 replay(input:CommercePostingSetupInput,key:string):Promise<CommercePostingSetupResult|null>;
 current():Promise<CommercePostingSetupConfiguration|null>;
 contextKey():Promise<string>;
 validate(input:CommercePostingSetupInput):Promise<void>;
 assertNoHistory():Promise<void>;
 commit(input:CommercePostingSetupInput,key:string):Promise<CommercePostingSetupResult>;
}
export function createCommercePostingSetup(ports:CommercePostingSetupPorts){
 return async(raw:unknown,key:string)=>{
  const input=commercePostingSetupSchema.parse(raw),prior=await ports.replay(input,key);
  if(prior)return prior;
  if(await ports.current())throw new BusinessRuleError(409,'COMMERCE_POSTING_ALREADY_CONFIGURED','The existing Company posting mapping is retained. Initial setup cannot replace it.');
  if(await ports.contextKey()!==input.contextKey)throw new BusinessRuleError(409,'COMMERCE_POSTING_CONTEXT_CHANGED','The selected work context or accounting references changed. Keep your draft and review the current setup.');
  await ports.validate(input);await ports.assertNoHistory();return ports.commit(input,key);
 };
}
