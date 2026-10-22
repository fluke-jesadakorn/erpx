import {BusinessRuleError} from '@erpx/server-shared/index';
import {manufacturingPostingSetupSchema,type ManufacturingPostingSetupInput,type ManufacturingPostingSetupResult} from '@erpx/server-commerce/domain/manufacturing-posting-setup';
export interface ManufacturingPostingSetupPorts{
 replay(input:ManufacturingPostingSetupInput,key:string):Promise<ManufacturingPostingSetupResult|null>;
 configured():Promise<boolean>;contextKey():string;
 validate(input:ManufacturingPostingSetupInput):Promise<void>;
 assertNoEconomics():Promise<void>;
 commit(input:ManufacturingPostingSetupInput,key:string):Promise<ManufacturingPostingSetupResult>;
}
/** Exact replay is history; the initial mapping never overwrites one already
 * configured and never silently adopts existing Stock/production economics. */
export function createManufacturingPostingSetup(ports:ManufacturingPostingSetupPorts){return async(raw:unknown,key:string)=>{
 const input=manufacturingPostingSetupSchema.parse(raw),prior=await ports.replay(input,key);if(prior)return prior;
 if(await ports.configured())throw new BusinessRuleError(409,'COMMERCE_POSTING_ALREADY_CONFIGURED','The existing Company posting mapping is retained. Initial setup cannot replace it.');
 if(input.contextKey!==ports.contextKey())throw new BusinessRuleError(409,'MANUFACTURING_POSTING_CONTEXT_CHANGED','The selected Company authority, reviewed defaults or original posting references changed. Keep the draft and review current setup.');
 await ports.assertNoEconomics();await ports.validate(input);return ports.commit(input,key);
};}
