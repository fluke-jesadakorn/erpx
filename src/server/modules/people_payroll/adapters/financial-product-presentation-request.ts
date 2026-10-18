import 'server-only';
import {headers} from 'next/headers';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {workspacePathParts} from '@/lib/work_projection/destination-registry';
import {productPageAccess} from '../../identity_access/public/product-workspace';
import type {WorkContext} from '../../identity_access/public/authorization';
import {peopleFinancialEntry,type PeopleFinancialEntry} from '../domain/financial-product-entry';
/** Host/path only selects presentation. The normal current selected role and
 * exact native owner entry decide the source flag again; no host grants IAM. */
export async function peopleFinancialSourcePresentation(actor:WorkContext,kind:PeopleFinancialEntry['kind']):Promise<boolean>{
 const request=await headers(),audience=productHostForHost(request.get('host'));
 if(audience!==(kind==='expense'?'finance':'people'))return false;
 const path=request.get('x-erpx-page-path');if(!path)return false;
 const destination=workspacePathParts(new URL(path,'https://entry.invalid').pathname)?.path??'';
 const target=peopleFinancialEntry(destination);if(target?.kind!==kind)return false;
 return (await productPageAccess(audience,actor.companyId,path)).sourceRecord;
}
