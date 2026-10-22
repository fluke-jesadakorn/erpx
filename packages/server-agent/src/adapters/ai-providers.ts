import {aiGatewayBootstrapSources} from '@erpx/server-manage_erpx/public/ai-gateway-sources';

import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {z} from "zod";
import {sha256} from "@erpx/server-runtime/public/crypto";
import type {StaffActor} from "@erpx/server-operations/public/staff-policy";
import {companyAuthority,authorityGuard} from "@erpx/server-operations/public/staff-policy";
import type {BetaEnvironment} from "@erpx/server-manage_erpx/public/beta-config";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {generateProviderJson,openAiKey,providerRequest,sealAiKey,type AiEndpoint} from "@erpx/server-agent/adapters/ai-transport";
import {isThaiHotelAiMockEnabled,thaiHotelAiMockId} from "@erpx/server-runtime/public/simulation-only-adapters";


import {BusinessRuleError} from '@erpx/server-shared/index';
import {createAiProviderApplication,type AiProviderRepository} from '@erpx/server-agent/application/providers';
import type {AiEndpoint as CoreEndpoint} from '@erpx/server-agent/domain/transport';
import {aiConnectionSchema,aiSelectionSchema,aiDiscoverySchema,type Connection,type Revision,type Selection,type AiProviderOverview,type AiProviderSnapshot} from '@erpx/server-agent/domain/providers';
export {aiConnectionSchema,aiVersionSchema,aiSelectionSchema,aiDiscoverySchema} from '@erpx/server-agent/domain/providers';
export type {AiConnectionView,AiProviderOverview,AiProviderSnapshot} from '@erpx/server-agent/domain/providers';
function nativeEndpoint(db:SqlDatabase,endpoint:CoreEndpoint):AiEndpoint{const {gateway,...rest}=endpoint;return gateway?{...rest,gateway:{...gateway,db}}:rest;}
async function external<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function exposedSync<T>(action:()=>T){try{return action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function application(db:SqlDatabase,env?:BetaEnvironment,generate=generateProviderJson){const repository:AiProviderRepository<SqlStatement>={async holdingBinding(){return aiGatewayBootstrapSources(db).binding();},
async connection(id:string,companyId:string){return db
    .prepare(
      "SELECT * FROM ai_connections WHERE id=? AND company_id=? AND deleted_at IS NULL",
    )
    .bind(id, companyId)
    .first<Connection>();},
async revision(id:string,value:number | null){return db
    .prepare(
      "SELECT * FROM ai_connection_revisions WHERE connection_id=? AND revision=?",
    )
    .bind(id, value)
    .first<Revision>();},
guardRevisionVersion(key:string,value:"ai-version",id:string,company_id:string,expected:number){return assertion(
    db,
    key,
    value,
    "EXISTS(SELECT 1 FROM ai_connections WHERE id=? AND company_id=? AND version=? AND deleted_at IS NULL)",
    [id, company_id, expected],
  );},
async connections(companyId:string){return (await db
      .prepare(
        "SELECT * FROM ai_connections WHERE company_id=? AND deleted_at IS NULL ORDER BY created_at,id",
      )
      .bind(companyId)
      .all<Connection>()).results;},
async selection(companyId:string){return db
    .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
    .bind(companyId)
    .first<Selection>();},
async usage(companyId:string,now:number){return (await db
      .prepare(
        "SELECT connection_id,model,COUNT(*) requests,SUM(input_tokens) input_tokens,SUM(output_tokens) output_tokens,SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END) failures FROM ai_provider_usage WHERE company_id=? AND created_at>=? GROUP BY connection_id,model",
      )
      .bind(companyId, now)
      .all<AiProviderOverview["usage"][number]>()).results;},
prepareConnectionUpdate(name:string,rev:number,now:number,recordId:string){return db
            .prepare(
              "UPDATE ai_connections SET name=?,draft_revision=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(name, rev, now, recordId);},
prepareConnectionCreate(recordId:string,companyId:string,name:string,userId:string,now:number){return db
            .prepare(
              "INSERT INTO ai_connections(id,company_id,name,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              companyId,
              name,
              userId,
              now,
              now,
            );},
prepareRevision(recordId:string,rev:number,baseUrl:string,authMode:"bearer" | "none",credential:string | null,textModel:string,value:string | null,userId:string,now:number,providerPreset:"custom" | "opencode-zen" | "opencode-go",textProtocol:"chat-completions" | "responses" | "messages",imageProtocol:"chat-completions" | "responses" | "messages"){return db
        .prepare(
          "INSERT INTO ai_connection_revisions(connection_id,revision,base_url,auth_mode,credential_json,text_model,image_model,created_by,created_at,provider_preset,text_protocol,image_protocol) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          recordId,
          rev,
          baseUrl,
          authMode,
          credential,
          textModel,
          value,
          userId,
          now,
          providerPreset,
          textProtocol,
          imageProtocol,
        );},
prepareCapabilityTest(now:number,value:0 | 1,value2:0 | 1,error:string | null,id:string,revision:number){return db
        .prepare(
          "UPDATE ai_connection_revisions SET tested_at=?,text_verified=?,image_verified=?,test_error=? WHERE connection_id=? AND revision=?",
        )
        .bind(
          now,
          value,
          value2,
          error,
          id,
          revision,
        );},
prepareTestVersion(now:number,id:string){return db
        .prepare(
          "UPDATE ai_connections SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id);},
guardTestedRevision(key:string,value:"tested-revision",id:string,draft_revision:number){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM ai_connection_revisions WHERE connection_id=? AND revision=? AND text_verified=1)",
        [id, draft_revision],
      );},
prepareActivation(now:number,id:string){return db
        .prepare(
          "UPDATE ai_connections SET active_revision=draft_revision,disabled=0,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id);},
async homeAiName(companyId:string){return db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?")
    .bind(companyId).first<{home_ai_name:string}>();},
guardSelectedCapability(key:string,value:string,id:string,companyId:string,version:number,value2:0 | 1){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM ai_connections c JOIN ai_connection_revisions r ON r.connection_id=c.id AND r.revision=c.active_revision WHERE c.id=? AND c.company_id=? AND c.version=? AND c.disabled=0 AND c.deleted_at IS NULL AND r.text_verified=1 AND (?=0 OR r.image_verified=1))",
        [id, companyId, version, value2],
      );},
guardSettingsVersion(key:string,value:"settings-version",companyId:string,expectedVersion:number){return assertion(
        db,
        key,
        value,
        "COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?",
        [companyId, expectedVersion],
      );},
prepareNameHistory(companyId:string,currentName:string){return db.prepare("INSERT INTO ai_home_name_history(company_id,name) VALUES(?,?) ON CONFLICT DO NOTHING").bind(companyId,currentName);},
prepareSelection(companyId:string,value:number,mode:"custom" | "deployment" | "disabled",value2:string | null,value3:string | null,imageMode:"custom" | "default" | "disabled",homeAiName:string,userId:string,now:number){return db
        .prepare(
          "INSERT INTO ai_company_settings(company_id,version,mode,text_connection_id,image_connection_id,image_mode,home_ai_name,updated_by,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,mode=excluded.mode,text_connection_id=excluded.text_connection_id,image_connection_id=excluded.image_connection_id,image_mode=excluded.image_mode,home_ai_name=excluded.home_ai_name,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
        )
        .bind(
          companyId,
          value,
          mode,
          value2,
          value3,
          imageMode,
          homeAiName,
          userId,
          now,
        );},
async disableSelection(companyId:string){return db
      .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
      .bind(companyId)
      .first<Selection>();},

prepareDisableUses(id:string,userId:string,now:number,companyId:string){return db
              .prepare(
                "UPDATE ai_company_settings SET mode=CASE WHEN text_connection_id=? THEN 'disabled' ELSE mode END,image_mode=CASE WHEN image_connection_id=? THEN 'disabled' ELSE image_mode END,version=version+1,updated_by=?,updated_at=? WHERE company_id=?",
              )
              .bind(id, id, userId, now, companyId);},
prepareDisableConnection(value:number | null,now:number,id:string){return db
        .prepare(
          "UPDATE ai_connections SET disabled=1,deleted_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(value, now, id);},
prepareDestroyCredentials(id:string){return db
              .prepare(
                "UPDATE ai_connection_revisions SET credential_json=NULL WHERE connection_id=?",
              )
              .bind(id);},
async currentSelection(companyId:string){return db
    .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
    .bind(companyId)
    .first<Selection>();},
prepareUsage(id:string,companyId:string,connectionId:string | null,revision:number,model:string,feature:string,status:"succeeded" | "failed",inputTokens:number,outputTokens:number,now:number,branchId:string | null,fundingSource:"unknown" | "simulation" | "byok" | "erpx"){return db
    .prepare(
      "INSERT INTO ai_provider_usage(id,company_id,connection_id,revision,model,feature,status,input_tokens,output_tokens,created_at,branch_id,funding_source) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(
      id,
      companyId,
      connectionId,
      revision,
      model,
      feature,
      status,
      inputTokens,
      outputTokens,
      now,
      branchId,
      fundingSource,
    );},
snapshotGuard(key:string,snapshot:AiProviderSnapshot,label:string,simulationSnapshot:boolean){
  const settings =
    "COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?";
  return assertion(
    db,
    key,
    label,
    `${settings}${snapshot.connectionId ? " AND EXISTS(SELECT 1 FROM ai_connections c JOIN ai_connection_revisions r ON r.connection_id=c.id AND r.revision=c.active_revision WHERE c.id=? AND c.company_id=? AND c.active_revision=? AND c.disabled=0 AND c.deleted_at IS NULL AND r.text_verified=1 AND (?='text' OR r.image_verified=1))" : simulationSnapshot ? "" : " AND COALESCE((SELECT mode FROM ai_company_settings WHERE company_id=?),'deployment')='deployment'"}`,
    [
      snapshot.companyId,
      snapshot.settingsVersion,
      ...(snapshot.connectionId
        ? [
            snapshot.connectionId,
            snapshot.companyId,
            snapshot.revision,
            snapshot.kind,
          ]
        : simulationSnapshot
          ? []
          : [snapshot.companyId]),
    ],
  );
}};return createAiProviderApplication({repository,clock:{now:()=>Date.now()},configuration:{production:process.env.NODE_ENV==='production',baseUrl:env?.AI_BASE_URL,model:env?.AI_MODEL,apiKey:env?.AI_API_KEY,keyStorageReady:Boolean(env?.AI_CREDENTIAL_KEYS),simulationModel:thaiHotelAiMockId},simulation:isThaiHotelAiMockEnabled,hash:sha256,sealKey:(company,id,key)=>external(()=>sealAiKey(env!,company,id,key)),openKey:(company,id,sealed)=>external(()=>openAiKey(env!,company,id,sealed)),generate:(endpoint,...args)=>external(()=>generate(nativeEndpoint(db,endpoint),...args)),request:(endpoint,path)=>external(()=>providerRequest(nativeEndpoint(db,endpoint),path)),authority:{has:(actor,roles)=>companyAuthority(db,actor,roles),guard:(key,actor,roles)=>authorityGuard(db,key,actor,roles)},commands:{replay:(...args)=>replayCommand(db,...args),commit:(...args)=>executeCommand(db,...args)}});}
export async function requireAiAdmin(db: SqlDatabase,actor: StaffActor){return exposed(()=>application(db).requireAiAdmin(actor));}
export async function aiProviderOverview(db: SqlDatabase,actor: StaffActor,env: BetaEnvironment){return exposed(()=>application(db,env).aiProviderOverview(actor));}
export async function saveAiConnection(db: SqlDatabase,actor: StaffActor,env: BetaEnvironment,id: string | null,input: z.infer<typeof aiConnectionSchema>,key: string){return exposed(()=>application(db,env).saveAiConnection(actor,id,input,key));}
export async function testAiConnection(db: SqlDatabase,actor: StaffActor,env: BetaEnvironment,id: string,expectedVersion: number,key: string,generate = generateProviderJson){return exposed(()=>application(db,env,generate).testAiConnection(actor,id,expectedVersion,key));}
export async function discoverAiModelsInput(db: SqlDatabase,actor: StaffActor,env: BetaEnvironment,input: z.infer<typeof aiDiscoverySchema>){return exposed(()=>application(db,env).discoverAiModelsInput(actor,input));}
export async function discoverAiModels(db: SqlDatabase,actor: StaffActor,env: BetaEnvironment,id: string){return exposed(()=>application(db,env).discoverAiModels(actor,id));}
export async function activateAiConnection(db: SqlDatabase,actor: StaffActor,id: string,expectedVersion: number,key: string){return exposed(()=>application(db).activateAiConnection(actor,id,expectedVersion,key));}
export async function selectAiProviders(db: SqlDatabase,actor: StaffActor,input: z.infer<typeof aiSelectionSchema>,key: string){return exposed(()=>application(db).selectAiProviders(actor,input,key));}
export async function disableAiConnection(db: SqlDatabase,actor: StaffActor,id: string,input: { expectedVersion: number; disableUses: boolean; remove: boolean },key: string){return exposed(()=>application(db).disableAiConnection(actor,id,input,key));}
export async function resolveAiProvider(db: SqlDatabase,companyId: string,env: BetaEnvironment,kind: "text" | "image",holdingSlug?: string,holdingId?: string){const result=await exposed(()=>application(db,env).resolveAiProvider(companyId,kind,holdingSlug,holdingId));return {...result,endpoint:nativeEndpoint(db,result.endpoint)};}
export function aiSnapshotGuard(db: SqlDatabase,key: string,snapshot: AiProviderSnapshot,label = "ai-provider-revision"){return exposedSync(()=>application(db).aiSnapshotGuard(key,snapshot,label));}
export function aiUsageStatement(db: SqlDatabase,id: string,snapshot: AiProviderSnapshot,feature: string,status: "succeeded" | "failed",inputTokens = 0,outputTokens = 0,branchId: string | null = null){return exposedSync(()=>application(db).aiUsageStatement(id,snapshot,feature,status,inputTokens,outputTokens,branchId));}

