import 'server-only';
import {createHash} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {IamActor} from '../domain/iam-actor';
import {assertCompanyScope} from './iam-context';
import {iamDb} from './iam-db';
import {iamCommand} from './iam-transactions';
import {requirePermission} from './iam-permissions';
import {managementCommand} from './user-management';
import {previewDirectoryChange,submitDirectoryChange} from './directory-operations';
import {directoryMappingPerson} from '../../people_payroll/public/directory-mapping-person';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {createDirectoryFieldMappingApplication,type DirectoryFieldFacts} from '../application/directory-field-mapping';
import type {FieldSyncInput,DirectoryFieldState,DirectoryFieldIdentity,DirectoryFieldValues,DirectoryFieldAttribute} from '../domain/directory-field-mapping';
export {fieldSyncInput} from '../domain/directory-field-mapping';
function facts(client:PoolClient):DirectoryFieldFacts{return {state:async(actor,connectionId)=>(await client.query<DirectoryFieldState>("SELECT s.*,c.version connection_version FROM people_directory_state s JOIN iam_connections c ON c.id=s.connection_id WHERE s.connection_id=$1 AND s.company_id=$2 AND c.status='active'",[connectionId,actor.companyId])).rows[0]??null,identities:async(actor,connectionId,personId)=>(await client.query<DirectoryFieldIdentity>("SELECT i.* FROM iam_identities i WHERE i.connection_id=$1 AND i.company_id=$2 AND i.user_id IS NOT NULL AND people_person_user($2,$3,i.user_id) AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=i.company_id AND m.user_id=i.user_id AND m.identity_id=i.id AND m.status='active')",[connectionId,actor.companyId,personId])).rows,person:(actor,personId)=>directoryMappingPerson(client,actor.companyId,personId),values:async(actor,personId)=>(await client.query<DirectoryFieldValues>("SELECT * FROM um_values WHERE company_id=$1 AND target='person' AND object_id=$2",[actor.companyId,personId])).rows[0]??null,attribute:async(actor,id)=>(await client.query<DirectoryFieldAttribute>('SELECT * FROM um_attributes WHERE company_id=$1 AND id=$2',[actor.companyId,id])).rows[0]??null};}
export async function mappedFields(db:PostgresDatabase,actor:IamActor,input:FieldSyncInput,key:string,previewOnly:boolean){const database=iamDb(db),app=createDirectoryFieldMappingApplication<Awaited<ReturnType<typeof managementCommand>>|Awaited<ReturnType<typeof submitDirectoryChange>>>({companyScope:assertCompanyScope,authorize:requirePermission,hash:value=>createHash('sha256').update(JSON.stringify(value)).digest('hex'),readSnapshot:async run=>{const client=await database.pool.connect();try{await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');return await run(facts(client));}finally{await client.query('ROLLBACK');client.release();}},authorizeCommand:(actor,key,input,run)=>iamCommand(database,actor,key,'directory.mapping.authorize',input,'iam.directory.manage',client=>run(facts(client))),previewValues:(actor,change,key)=>managementCommand(database,actor,change,key,true),applyValues:(actor,change,key,token)=>managementCommand(database,actor,change,key,false,token),previewDirectory:(actor,change)=>previewDirectoryChange(database,actor,change),submitDirectory:(actor,change,token,key)=>submitDirectoryChange(database,actor,change,token,key)});try{return await app.mappedFields(actor,input,key,previewOnly);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
