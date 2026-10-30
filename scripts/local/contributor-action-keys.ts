import {randomBytes} from 'node:crypto';
import {existsSync,linkSync,lstatSync,readFileSync,unlinkSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';

export type ContributorActionApp='account'|'apps'|'service';
const apps:ContributorActionApp[]=['account','apps','service'];
export function readContributorActionKey(config:string,id:ContributorActionApp,uid=0){
 const path=join(config,`next-actions-${id}.key`),st=lstatSync(path);
 if(!st.isFile()||st.isSymbolicLink()||(st.mode&0o077)!==0||st.uid!==uid)throw Error('CONTRIBUTOR_ACTION_KEY_INVALID');
 const value=readFileSync(path,'utf8');if(!/^[A-Za-z0-9+/]{43}=$/.test(value)||Buffer.from(value,'base64').length!==32||Buffer.from(value,'base64').toString('base64')!==value)throw Error('CONTRIBUTOR_ACTION_KEY_INVALID');return value;
}
/** Called only after the keyed workspace and container-private start lock are verified. */
export function prepareContributorActionKeys(config:string,uid=0){
 const keys:string[]=[];
 for(const id of apps){const path=join(config,`next-actions-${id}.key`);
  if(!existsSync(path)){const tmp=`${path}.${randomBytes(16).toString('hex')}.tmp`;try{writeFileSync(tmp,randomBytes(32).toString('base64'),{mode:0o600,flag:'wx'});try{linkSync(tmp,path);}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}}finally{if(existsSync(tmp))unlinkSync(tmp);}}
  keys.push(readContributorActionKey(config,id,uid));
 }
 if(new Set(keys).size!==apps.length)throw Error('CONTRIBUTOR_ACTION_KEY_INVALID');
}
