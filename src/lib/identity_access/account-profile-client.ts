import type {AccountPreferences,AccountPreferenceSnapshot,AccountProfile} from '@/server/modules/identity_access/public/account-preferences';

function preferences(value:unknown):value is AccountPreferences{
 return !!value&&typeof value==='object'&&!Array.isArray(value)&&'language'in value&&(value.language==='en'||value.language==='th')
  &&'appearance'in value&&(value.appearance==='system'||value.appearance==='light'||value.appearance==='dark')
  &&'version'in value&&typeof value.version==='number'&&Number.isSafeInteger(value.version)&&value.version>=0;
}
/** A successful transport response is not yet a valid native profile outcome. */
export function accountProfileAcknowledgement(value:unknown):AccountProfile{
 const profile=value&&typeof value==='object'&&!Array.isArray(value)&&'data'in value?value.data:undefined;
 if(!profile||typeof profile!=='object'||Array.isArray(profile)||!('name'in profile)||typeof profile.name!=='string'
  ||!('email'in profile)||typeof profile.email!=='string'||!('preferences'in profile)||!preferences(profile.preferences))
  throw Error('Could not read your saved account settings. Load saved settings before trying again.');
 return{name:profile.name,email:profile.email,preferences:{language:profile.preferences.language,appearance:profile.preferences.appearance,version:profile.preferences.version}};
}
export function accountProfilePreferenceEvent(value:unknown):AccountPreferenceSnapshot|undefined{
 if(!value||typeof value!=='object'||Array.isArray(value)||!('key'in value)||typeof value.key!=='string'
  ||!('preference'in value)||!preferences(value.preference))return;
 return{key:value.key,preference:value.preference};
}
export type AccountProfileWitness={accountKey:string;version:number;foreignAccount:boolean;pending:boolean};
/** Version ordering is local to one rendered account, never shared globally. */
export function observeAccountProfileWitness(prior:AccountProfileWitness,accountKey:string,serverVersion:number,peer:(AccountPreferenceSnapshot&{pending?:boolean})|null|undefined):AccountProfileWitness{
 const version=Math.max(serverVersion,prior.accountKey===accountKey?prior.version:0);
 return{accountKey,version:peer?.key===accountKey&&peer.preference?Math.max(version,peer.preference.version):version,
  foreignAccount:!!peer&&/^[a-f0-9]{64}$/.test(peer.key)&&peer.key!==accountKey,
  // Saved events carry no pending field; only an actual owner state can clear
  // its newer in-flight intent. Profile completion must never cancel it.
  pending:peer?.key===accountKey&&typeof peer.pending==='boolean'?peer.pending:prior.accountKey===accountKey&&prior.pending};
}
export function accountProfileResponseCurrent(witness:AccountProfileWitness,accountKey:string,profile:AccountProfile){
 return witness.accountKey===accountKey&&!witness.foreignAccount&&!witness.pending&&profile.preferences.version>=witness.version;
}
