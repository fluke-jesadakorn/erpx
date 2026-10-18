'use client';
import {Field} from '@/components/ui/field';
import {Select} from '@/components/ui/input';
import {useContextSwitch} from '@/components/workplace/context-switch-context';
import {useProviderContext} from './provider-context-boundary';
import {useAccountPreferences} from './account-preferences-context';
export function ProviderContextSwitcher(){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,en:string)=>th?thai:en;
 const {state,select}=useProviderContext(),{busy}=useContextSwitch();
 return <Field appearance='standard'><span>{t('บทบาทผู้ให้บริการ','Provider role')}</span><Select appearance='standard' aria-label={t('บทบาทผู้ให้บริการ','Provider role')} value={state.seed.selectedId} disabled={busy||state.pending||state.retry||state.stale} onChange={event=>{if(event.target.value!==state.seed.selectedId)void select(event.target.value).catch(()=>{});}}>{state.seed.choices.map(choice=><option key={choice.id} value={choice.id}>{choice.label}</option>)}</Select></Field>;
}
