import 'server-only';
import {iamDb} from '../../identity_access/public/iam-db';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {executeCommand,replayCommand} from '../../workflow/public/commands';
import type {RosterCommands} from '../application/people-roster';
import {workerStartupSourcePolicy,type WorkerStartupConsumer,type ProjectWorkerStartupProof,type ProjectWorkerPlacementProof} from './project-worker-startup-source-sql';
const units=new WeakMap<SqlDatabase,{identity:string;consumer:WorkerStartupConsumer}>();
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null]);
type Binding={key:string;operation:'people.directory.create'|'people.directory.update'|'people.lifecycle.submit';input:unknown;marker:'_projectWorkerStartup'|'_projectWorkerPlacement'|'_learningWorkerStartup'|'_learningWorkerPlacement';proof:ProjectWorkerStartupProof|ProjectWorkerPlacementProof};
const commands=new WeakMap<SqlDatabase,Binding>();
type OwnedSelfDirectory={key:string;operation:'people.directory.create'|'people.directory.update';proof:ProjectWorkerStartupProof;image:string|null};
const ownedSelfDirectory=new WeakMap<SqlDatabase,OwnedSelfDirectory>();
/** Actual post-result identity rows only. This is an expected owned effect,
 * never an acting WorkContext or a relaxation of current IAM authority. */
export function projectWorkerSelfDirectoryImage(database:SqlDatabase,actor:WorkContext,proof:ProjectWorkerStartupProof){return database.prepare(`SELECT jsonb_build_object('person',(SELECT to_jsonb(p) FROM hr_people p WHERE p.id=? AND p.company_id=? AND p.user_id=?),'engagement',(SELECT to_jsonb(e) FROM hr_engagements e WHERE e.id=? AND e.person_id=? AND e.company_id=?))::text image`).bind(proof.personId,actor.companyId,actor.userId,proof.engagementId,proof.personId,actor.companyId).first<string>('image');}
export function projectWorkerOwnedSelfDirectory(database:SqlDatabase,actor:WorkContext){requireProjectWorkerUnit(database,actor);return ownedSelfDirectory.get(database);}
/** Private owning unit only. No public API accepts a client or run callback. */
export function registerProjectWorkerUnit(database:SqlDatabase,actor:WorkContext,consumer:WorkerStartupConsumer='projects'){const aliases=[database,iamDb(database)];if(aliases.some(alias=>units.has(alias)))throw new ApiError(503,'PROJECT_WORKER_UNIT_MISMATCH','The worker starter requires its own Holding unit.');for(const alias of aliases)units.set(alias,{identity:identity(actor),consumer});return()=>{for(const alias of aliases){commands.delete(alias);ownedSelfDirectory.delete(alias);units.delete(alias);}};}
export function requireProjectWorkerUnit(database:SqlDatabase,actor:WorkContext){if(units.get(database)?.identity!==identity(actor))throw new ApiError(503,'PROJECT_WORKER_UNIT_REQUIRED','The original worker command requires its owned Holding unit.');}
export async function withProjectWorkerNativeCommand<Result>(database:SqlDatabase,actor:WorkContext,binding:Binding,run:()=>Promise<Result>){requireProjectWorkerUnit(database,actor);const policy=workerStartupSourcePolicy(units.get(database)!.consumer);if((binding.proof.kind!==policy.startupKind&&binding.proof.kind!==policy.placementKind)||(binding.marker!==policy.startupMarker&&binding.marker!==policy.placementMarker))throw new ApiError(503,'PROJECT_WORKER_UNIT_MISMATCH','The fixed worker source cannot borrow another consumer’s proof.');if(commands.has(database))throw new ApiError(503,'PROJECT_WORKER_UNIT_MISMATCH','A different native command cannot borrow this unit.');const aliases=[...new Set([database,iamDb(database)])];for(const alias of aliases)commands.set(alias,binding);try{const result=await run();if((binding.proof.kind==='project-worker-startup'||binding.proof.kind==='learning-worker-startup')&&binding.proof.accountId===actor.userId&&actor.iam?.personId===null&&binding.operation!=='people.lifecycle.submit'){const proof={...binding.proof},image=await projectWorkerSelfDirectoryImage(database,actor,proof),effect={key:binding.key,operation:binding.operation,proof,image};for(const alias of aliases){if(ownedSelfDirectory.has(alias))throw new ApiError(503,'PROJECT_WORKER_UNIT_MISMATCH','A second self Directory result cannot replace this unit’s original effect.');ownedSelfDirectory.set(alias,effect);}}return result;}finally{for(const alias of aliases)commands.delete(alias);}}
export function requireProjectWorkerNativeCommand(database:SqlDatabase,actor:WorkContext,key:string,operation:string){requireProjectWorkerUnit(database,actor);const binding=commands.get(database);if(!binding||binding.key!==key||binding.operation!==operation)throw new ApiError(503,'PROJECT_WORKER_UNIT_MISMATCH','The original native command binding changed.');return binding;}
/** Original operations, IDs, command fingerprinting and receipt/audit/outbox
 * machinery. Only the closed registered native command captures its source
 * envelope and immutable private result marker. Ordinary behavior is exact. */
export function projectWorkerNativeCommands(database:SqlDatabase):RosterCommands<SqlStatement>{return {
 replay:(actor,key,operation,id,input)=>{const binding=commands.get(database);if(binding)requireProjectWorkerNativeCommand(database,actor,key,operation);return replayCommand(database,actor,key,operation,id,binding?.input??input);},
 commit:(actor,key,operation,input,id,version,result,effects,scope)=>{const binding=commands.get(database);if(binding)requireProjectWorkerNativeCommand(database,actor,key,operation);return executeCommand(database,actor,key,operation,binding?.input??input,id,version,binding?Object.assign({},result,{[binding.marker]:binding.proof}):result,effects,scope);},
};}
