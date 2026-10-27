import 'server-only';
import {headers} from 'next/headers';
import type {NativeJournalDetailRead,NativeJournalReadInput} from './native-journal-detail';
/** A single actual Next request snapshot. No preliminary session query, actor
 * construction, cross-request cache or current-page switch occurs here. */
export function createNextNativeJournalDetailReader(read:NativeJournalDetailRead){
 return async(input:NativeJournalReadInput,signal?:AbortSignal)=>read(await headers(),input,signal);
}
