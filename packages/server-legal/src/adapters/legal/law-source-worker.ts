import "server-only";
import type {SqlDatabase,FileStore} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {legalSourceNotification} from "@erpx/server-collaboration/public/legal-law-source-notifications";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {legalRuntimePredicate,legalCompanyCapabilityReady} from "@erpx/server-legal/adapters/legal/runtime-gate";
import {fetchOfficialLawSource} from "@erpx/server-legal/adapters/legal/law-source-provider";
import {createLegalLawSourceWorker,type OfficialSourceJob} from "@erpx/server-legal/application/law-source-worker";
export function drainLegalSourceChecks(db:SqlDatabase,files:FileStore,fetcher=fetchOfficialLawSource,now=Date.now()){return createLegalLawSourceWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,error:(status,code,message)=>new ApiError(status,code,message),ready:id=>legalCompanyCapabilityReady(db,id,"sources"),storage:files,fetch:async url=>{const response=await fetcher(url);return {ok:response.ok,status:response.status,contentType:()=>response.headers.get("content-type"),bytes:()=>response.arrayBuffer()};},repository:{
 schedule:async(date,now)=>{await db.prepare(`INSERT INTO legal_law_source_checks(id,source_id,company_id,rule_id,check_date,created_at) SELECT 'law-check-'||md5(s.id||?),s.id,s.company_id,s.rule_id,?,? FROM legal_law_sources s WHERE s.state='active' AND ${legalRuntimePredicate("s.company_id", "sources")} ON CONFLICT(source_id,check_date) DO NOTHING`).bind(date,date,now).run();},
 recover:async now=>{await db.prepare("UPDATE legal_law_source_checks SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,error='LEGAL_SOURCE_CHECK_INTERRUPTED',lease_token=NULL,lease_until=NULL WHERE state='running' AND lease_until<?").bind(now).run();},
 claim:(token,until)=>db.prepare(`UPDATE legal_law_source_checks SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT c.id FROM legal_law_source_checks c JOIN legal_law_sources s ON s.id=c.source_id WHERE c.state='queued' AND c.attempts<3 AND s.state='active' AND ${legalRuntimePredicate("c.company_id", "sources")} ORDER BY c.created_at FOR UPDATE OF c SKIP LOCKED LIMIT 1) RETURNING *`).bind(token,until).first<OfficialSourceJob>(),
 source:(id,company)=>db.prepare("SELECT source_url FROM legal_law_sources WHERE id=? AND company_id=? AND state='active'").bind(id,company).first<{source_url:string}>(),
 unchanged:async(sourceId,id,checksum)=>Boolean(await db.prepare("SELECT 1 FROM legal_law_source_checks WHERE source_id=? AND id<>? AND checksum=? AND state IN ('unchanged','pending_review','reviewed') LIMIT 1").bind(sourceId,id,checksum).first()),
 complete:async(job,token,checksum,objectKey,unchanged,now)=>{
    await db.batch([db.prepare("INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) VALUES(?,'official-source',?,?,?,'system:legal-source',?) ON CONFLICT DO NOTHING").bind(job.rule_id, job.id, objectKey, checksum, now), db.prepare("UPDATE legal_law_source_checks SET state=?,checked_at=?,checksum=?,object_key=?,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND state='running' AND lease_token=?").bind(unchanged ? "unchanged" : "pending_review", now, checksum, objectKey, job.id, token), legalSourceNotification(db,{identifier:`law-source-notice-${job.id}`,now,ruleId:job.rule_id,unchanged})]);

},failed:async(id,token,error)=>{await db.prepare("UPDATE legal_law_source_checks SET state='failed',checked_at=NULL,error=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?").bind(error,id,token).run();}
}})(now);}
