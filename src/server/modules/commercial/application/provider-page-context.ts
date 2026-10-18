import type {ProviderContextSeed} from '@/lib/identity_access/provider-context';
/** Bound display flags to the same exact Provider revision as the SSR picker. */
export function createProviderCommercialPageContext<Overview>(ports:{context():Promise<ProviderContextSeed>;overview():Promise<Overview>;changed():Error}){
 return async()=>{const before=await ports.context(),overview=await ports.overview(),context=await ports.context();if(before.contextKey!==context.contextKey||before.accountKey!==context.accountKey)throw ports.changed();return {context,overview};};
}
