import {BusinessRuleError} from '../../shared';
import {posPostingSetupSchema,type PosPostingSetupInput,type PosPostingSetupResult} from '../domain/pos-posting-setup';
export interface PosPostingSetupPorts{
 replay(input:PosPostingSetupInput,key:string):Promise<PosPostingSetupResult|null>;
 configured():Promise<boolean>;contextKey():string;
 validate(input:PosPostingSetupInput):Promise<void>;
 assertNoEconomics():Promise<void>;
 commit(input:PosPostingSetupInput,key:string):Promise<PosPostingSetupResult>;
}
/** Exact replay is history; the initial mapping never overwrites one already
 * configured and never silently adopts existing Stock/POS economics. */
export function createPosPostingSetup(ports:PosPostingSetupPorts){return async(raw:unknown,key:string)=>{
 const input=posPostingSetupSchema.parse(raw),prior=await ports.replay(input,key);if(prior)return prior;
 if(await ports.configured())throw new BusinessRuleError(409,'COMMERCE_POSTING_ALREADY_CONFIGURED','The existing Company posting mapping is retained. Initial setup cannot replace it.');
 if(input.contextKey!==ports.contextKey())throw new BusinessRuleError(409,'POS_POSTING_CONTEXT_CHANGED','The selected Company authority, reviewed defaults or original posting references changed. Keep the draft and review current setup.');
 await ports.assertNoEconomics();await ports.validate(input);return ports.commit(input,key);
};}
