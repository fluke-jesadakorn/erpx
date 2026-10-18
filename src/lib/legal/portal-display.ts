import type {QuickAccessPreference} from '@/lib/work_projection/quick-access';
/** Retained share rows contain owner identities and original-document metadata.
 * The initial Client list consumes only these display values. */
export function legalPortalSharesDisplay(rows:readonly Record<string,unknown>[]){
 return rows.map(({id,name,version,expires_at})=>({id,name,version,expires_at}));
}
export function legalPortalPreferenceDisplay(value:QuickAccessPreference):QuickAccessPreference{
 return {pinnedIds:value.pinnedIds===null?null:[...value.pinnedIds],version:value.version};
}
