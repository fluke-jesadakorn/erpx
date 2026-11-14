import 'server-only';
import {createHash} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import {assertCompanyScope} from '@erpx/server-identity_access/adapters/iam-context';
import {iamDb} from '@erpx/server-identity_access/adapters/iam-db';
import {iamCommand} from '@erpx/server-identity_access/adapters/iam-transactions';
import {requirePermission} from '@erpx/server-identity_access/adapters/iam-permissions';
import {nativeUserManagementAvailable,nativeUserManagementPreviewForHolding,nativeUserManagementSaveForHolding} from '@erpx/server-identity_access/adapters/native-user-management';
import {nativeDirectoryOperationsAvailable,nativeDirectoryOperationsPreviewForHolding,nativeDirectoryOperationsSubmitForHolding} from '@erpx/server-identity_access/adapters/native-directory-operations';
import {directoryMappingPerson} from '@erpx/server-people_payroll/public/directory-mapping-person';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createDirectoryFieldMappingApplication,type DirectoryFieldFacts} from '@erpx/server-identity_access/application/directory-field-mapping';
import type {FieldSyncInput,DirectoryFieldState,DirectoryFieldIdentity,DirectoryFieldValues,DirectoryFieldAttribute} from '@erpx/server-identity_access/domain/directory-field-mapping';
export {fieldSyncInput} from '@erpx/server-identity_access/domain/directory-field-mapping';
function facts(client:PoolClient):DirectoryFieldFacts{return {state:async(actor,connectionId)=>(await client.query<DirectoryFieldState>("SELECT s.*,c.version connection_version FROM people_directory_state s JOIN iam_connections c ON c.id=s.connection_id WHERE s.connection_id=$1 AND s.company_id=$2 AND c.status='active'",[connectionId,actor.companyId])).rows[0]??null,identities:async(actor,connectionId,personId)=>(await client.query<DirectoryFieldIdentity>("SELECT i.* FROM iam_identities i WHERE i.connection_id=$1 AND i.company_id=$2 AND i.user_id IS NOT NULL AND people_person_user($2,$3,i.user_id) AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=i.company_id AND m.user_id=i.user_id AND m.identity_id=i.id AND m.status='active')",[connectionId,actor.companyId,personId])).rows,person:(actor,personId)=>directoryMappingPerson(client,actor.companyId,personId),values:async(actor,personId)=>(await client.query<DirectoryFieldValues>("SELECT * FROM um_values WHERE company_id=$1 AND target='person' AND object_id=$2",[actor.companyId,personId])).rows[0]??null,attribute:async(actor,id)=>(await client.query<DirectoryFieldAttribute>('SELECT * FROM um_attributes WHERE company_id=$1 AND id=$2',[actor.companyId,id])).rows[0]??null};}
type MappingValuesTransport={
  preview(actor:IamActor,change:unknown,key:string):Promise<{previewToken:string}>;
  save(actor:IamActor,change:unknown,key:string,token:string):Promise<unknown>;
};
let testValuesTransport:MappingValuesTransport|undefined;
/** Test seam for the directory mapping oracle; production always uses the native transport. */
export function setManagementValuesTransportForTest(value?:MappingValuesTransport){testValuesTransport=value;}

export async function mappedFields(db:PostgresDatabase,actor:IamActor,input:FieldSyncInput,key:string,previewOnly:boolean,headers?:Headers){const database=iamDb(db),app=createDirectoryFieldMappingApplication<Awaited<ReturnType<typeof nativeUserManagementPreviewForHolding>>|Awaited<ReturnType<typeof nativeDirectoryOperationsSubmitForHolding>>>({companyScope:assertCompanyScope,authorize:requirePermission,hash:value=>createHash('sha256').update(JSON.stringify(value)).digest('hex'),readSnapshot:async run=>{const client=await database.pool.connect();try{await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');return await run(facts(client));}finally{await client.query('ROLLBACK');client.release();}},authorizeCommand:(actor,key,input,run)=>iamCommand(database,actor,key,'directory.mapping.authorize',input,'iam.directory.manage',client=>run(facts(client))),previewValues:(actor,change,key)=>{
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
