import 'server-only';
import {createHash} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import type {IamActor} from '@erpx/identity-access-server/domain/iam-actor';
import {assertCompanyScope} from '@erpx/identity-access-server/adapters/iam-context';
import {iamDb} from '@erpx/identity-access-server/adapters/iam-db';
import {iamCommand} from '@erpx/identity-access-server/adapters/iam-transactions';
import {requirePermission} from '@erpx/identity-access-server/adapters/iam-permissions';
import {nativeUserManagementAvailable,nativeUserManagementPreviewForHolding,nativeUserManagementReadForHolding,nativeUserManagementSaveForHolding} from '@erpx/identity-access-server/adapters/native-user-management';
import {nativeDirectoryOperationsAvailable,nativeDirectoryOperationsPreviewForHolding,nativeDirectoryOperationsSubmitForHolding} from '@erpx/identity-access-server/adapters/native-directory-operations';
import {directoryMappingPerson} from '@erpx/people-payroll-server/public/directory-mapping-person';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createDirectoryFieldMappingApplication,type DirectoryFieldFacts} from '@erpx/identity-access-server/application/directory-field-mapping';
import type {FieldSyncInput,DirectoryFieldState,DirectoryFieldIdentity,DirectoryFieldValues,DirectoryFieldAttribute} from '@erpx/identity-access-server/domain/directory-field-mapping';
export {fieldSyncInput} from '@erpx/identity-access-server/domain/directory-field-mapping';

type NativeFacts=Pick<DirectoryFieldFacts,'values'|'attribute'>;
function invalid():never{throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','The native user-management reply was invalid.');}
function row(value:unknown):Record<string,unknown>{if(!value||typeof value!=='object'||Array.isArray(value))invalid();return value as Record<string,unknown>;}
function version(value:unknown):number{if(!Number.isSafeInteger(value)||(value as number)<0)invalid();return value as number;}
/** Project the native management snapshot into the two mapping read facts.
 * No SQL: the native owner GET is authoritative for values and attributes. */
export function fieldFactsFromSnapshot(snapshot:unknown):NativeFacts{
  const data=row(snapshot);if(!Array.isArray(data.values)||!Array.isArray(data.attributes))invalid();
  const values=data.values.map(row).map(v=>{if(!v.values||typeof v.values!=='object'||Array.isArray(v.values))invalid();return {target:v.target,objectId:v.objectId,version:version(v.version),definition:v.values as Record<string,unknown>};});
  const attributes=data.attributes.map(row).map(a=>{if(typeof a.id!=='string')invalid();const {id,version:foundVersion,canView:_view,canEdit:_edit,...definition}=a;return {id,version:version(foundVersion),definition:definition as DirectoryFieldAttribute['definition']};});
  return {
    values:async(_actor,personId)=>{const found=values.find(v=>v.target==='person'&&v.objectId===personId);return found?{version:found.version,definition:found.definition}:null;},
    attribute:async(_actor,id)=>attributes.find(a=>a.id===id)??null,
  };
}
let testFieldFacts:NativeFacts|undefined;
/** Test seam for the native mapping read oracle; production reads the owner GET. */
export function setFieldFactsForTest(value?:NativeFacts){testFieldFacts=value;}
async function nativeFacts(actor:IamActor,headers?:Headers):Promise<NativeFacts>{
  if(testFieldFacts)return testFieldFacts;
  if(!headers||!nativeUserManagementAvailable())throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires the native user-management service.');
  if(!actor.holdingId)throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires a Holding context.');
  return fieldFactsFromSnapshot(await nativeUserManagementReadForHolding(headers,actor.holdingId));
}
function facts(client:PoolClient,native:NativeFacts):DirectoryFieldFacts{return {state:async(actor,connectionId)=>(await client.query<DirectoryFieldState>("SELECT s.*,c.version connection_version FROM people_directory_state s JOIN iam_connections c ON c.id=s.connection_id WHERE s.connection_id=$1 AND s.company_id=$2 AND c.status='active'",[connectionId,actor.companyId])).rows[0]??null,identities:async(actor,connectionId,personId)=>(await client.query<DirectoryFieldIdentity>("SELECT i.* FROM iam_identities i WHERE i.connection_id=$1 AND i.company_id=$2 AND i.user_id IS NOT NULL AND people_person_user($2,$3,i.user_id) AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=i.company_id AND m.user_id=i.user_id AND m.identity_id=i.id AND m.status='active')",[connectionId,actor.companyId,personId])).rows,person:(actor,personId)=>directoryMappingPerson(client,actor.companyId,personId),values:native.values,attribute:native.attribute};}
type MappingValuesTransport={
  preview(actor:IamActor,change:unknown,key:string):Promise<{previewToken:string}>;
  save(actor:IamActor,change:unknown,key:string,token:string):Promise<unknown>;
};
let testValuesTransport:MappingValuesTransport|undefined;
/** Test seam for the directory mapping oracle; production always uses the native transport. */
export function setManagementValuesTransportForTest(value?:MappingValuesTransport){testValuesTransport=value;}

export async function mappedFields(db:PostgresDatabase,actor:IamActor,input:FieldSyncInput,key:string,previewOnly:boolean,headers?:Headers){const database=iamDb(db),native=await nativeFacts(actor,headers),app=createDirectoryFieldMappingApplication<Awaited<ReturnType<typeof nativeUserManagementPreviewForHolding>>|Awaited<ReturnType<typeof nativeDirectoryOperationsSubmitForHolding>>>({companyScope:assertCompanyScope,authorize:requirePermission,hash:value=>createHash('sha256').update(JSON.stringify(value)).digest('hex'),readSnapshot:async run=>{const client=await database.pool.connect();try{await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');return await run(facts(client,native));}finally{await client.query('ROLLBACK');client.release();}},authorizeCommand:(actor,key,input,run)=>iamCommand(database,actor,key,'directory.mapping.authorize',input,'iam.directory.manage',client=>run(facts(client,native))),previewValues:(actor,change,key)=>{
      if(testValuesTransport)return testValuesTransport.preview(actor,change,key);
      if(!headers||!nativeUserManagementAvailable())throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires the native user-management service.');
      if(!actor.holdingId)throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires a Holding context.');
      return nativeUserManagementPreviewForHolding(headers,actor.holdingId,change) as Promise<{previewToken:string}>;
    },applyValues:(actor,change,key,token)=>{
      if(testValuesTransport)return testValuesTransport.save(actor,change,key,token);
      if(!headers||!nativeUserManagementAvailable())throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires the native user-management service.');
      if(!actor.holdingId)throw new ApiError(503,'NATIVE_USER_MANAGEMENT_REQUIRED','Directory field mapping requires a Holding context.');
      return nativeUserManagementSaveForHolding(headers,actor.holdingId,change,token,key);
    },previewDirectory:(actor,change)=>{
      if(!headers||!nativeDirectoryOperationsAvailable()||!actor.holdingId)throw new ApiError(503,'NATIVE_DIRECTORY_OPERATION_REQUIRED','Directory field mapping requires the native command service.');
      return nativeDirectoryOperationsPreviewForHolding(headers,actor.holdingId,change) as Promise<{token:string}>;
    },submitDirectory:(actor,change,token,key)=>{
      if(!headers||!nativeDirectoryOperationsAvailable()||!actor.holdingId)throw new ApiError(503,'NATIVE_DIRECTORY_OPERATION_REQUIRED','Directory field mapping requires the native command service.');
      return nativeDirectoryOperationsSubmitForHolding(headers,actor.holdingId,{...change,previewToken:token},key);
    }});try{return await app.mappedFields(actor,input,key,previewOnly);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
