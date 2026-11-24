import "server-only";
import type {SqlDatabase,FileStore} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {legalSourceNotification} from "@erpx/server-collaboration/adapters/native-legal-law-source-notification";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {legalRuntimePredicate,legalCompanyCapabilityReady} from "@erpx/server-legal/adapters/legal/runtime-gate";
import {fetchOfficialLawSource} from "@erpx/server-legal/adapters/legal/law-source-provider";
import {createLegalLawSourceWorker,type OfficialSourceJob} from "@erpx/server-legal/application/law-source-worker";
import {legalWave3Statement} from "../native-legal-wave3";
import {legalStatePointSql} from "../legal-read-tail-r33-sources";
import {legalLawSourceCheckUnchangedSql} from '../legal-read-tail-r56-sources';
type LawSourceNoticeFacts={identifier:string;now:number;ruleId:string;unchanged:boolean};
export function drainLegalSourceChecks(db:SqlDatabase,files:FileStore,systemHolding:{holdingId:string;holdingSlug:string;holdingName:string},fetcher=fetchOfficialLawSource,now=Date.now(),notify?:(database:SqlDatabase,companyId:string,facts:LawSourceNoticeFacts)=>Promise<SqlStatement>){
 const scope={kind:'holding' as const,id:systemHolding.holdingId};
 const actorFor=(companyId:string):WorkContext=>({userId:"system:legal-source",name:"Legal source worker",email:"system@erpx.local",assignmentId:"system:legal-source",role:"system",companyId,branchId:null,departmentId:null,holdingId:systemHolding.holdingId as WorkContext["holdingId"],holdingSlug:systemHolding.holdingSlug,holdingName:systemHolding.holdingName,scope:{kind:"holding",holdingId:systemHolding.holdingId,holdingSlug:systemHolding.holdingSlug,holdingName:systemHolding.holdingName,companyId},assignmentScope:"holding",inherited:false,modules:[],assignments:[],workspaces:[]});const notifyFn=notify??((database:SqlDatabase,companyId:string,facts:LawSourceNoticeFacts)=>legalSourceNotification(database,actorFor(companyId),facts));return createLegalLawSourceWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,error:(status,code,message)=>new ApiError(status,code,message),ready:id=>legalCompanyCapabilityReady(db,id,"sources"),storage:files,fetch:async url=>{const response=await fetcher(url);return {ok:response.ok,status:response.status,contentType:()=>response.headers.get("content-type"),bytes:()=>response.arrayBuffer()};},repository:{
 schedule:async(date,now)=>{await (await legalWave3Statement(db,scope,'legal_source_schedule',[date,date,now,legalRuntimePredicate("s.company_id","sources")],`legal-source-schedule:${date}`,now)).run();},
 recover:async now=>{await (await legalWave3Statement(db,scope,'legal_source_recover',[now],`legal-source-recover:${now}`,now)).run();},
 claim:async(token,until)=>(await legalWave3Statement(db,scope,'legal_source_claim',[token,until,legalRuntimePredicate("c.company_id","sources")],`legal-source-claim:${token}`,Date.now())).first<OfficialSourceJob>(),
 source:(id,company)=>db.prepare(legalStatePointSql("source_url","legal_law_sources","company_id","active")).bind(id,company).first<{source_url:string}>(),
 unchanged:async(sourceId,id,checksum)=>Boolean(await db.prepare(legalLawSourceCheckUnchangedSql()).bind(sourceId,id,checksum).first()),
 complete:async(job,token,checksum,objectKey,unchanged,now)=>{
    await db.batch([
      await legalWave3Statement(db,scope,'legal_source_preserve',[job.rule_id,job.id,objectKey,checksum,now],`legal-source-preserve:${job.id}`,now),
      await legalWave3Statement(db,scope,'legal_source_complete',[unchanged?"unchanged":"pending_review",now,checksum,objectKey,job.id,token],`legal-source-complete:${job.id}`,now),
      await notifyFn(db,job.company_id,{identifier:`law-source-notice-${job.id}`,now,ruleId:job.rule_id,unchanged})]);

},failed:async(id,token,error)=>{await (await legalWave3Statement(db,scope,'legal_source_fail',[error,id,token],`legal-source-fail:${id}`,Date.now())).run();}
}})(now);}
