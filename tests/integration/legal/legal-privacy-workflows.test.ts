import {beforeAll,beforeEach,expect,it} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {legalCommand,detail} from "../../../src/server/modules/legal/public/legal/service";
import {access} from "../../../src/server/modules/legal/public/legal/access";
import {featureCommand,featureDetail} from "../../../src/server/modules/legal/public/legal/features";
const db=env.H001_DB,key=()=>crypto.randomUUID();
let f:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));
beforeEach(async()=>{f=await seedExpenseCompany(db,env.H001_FILES);f.manager=await f.member("privacy-owner","legal_manager");f.tie=await f.member("privacy-reviewer","legal_counsel");await legalCommand(db,f.manager,{action:"settings",enabled:true},key());});
async function fixture(data:Record<string,string>){
 const result=await legalCommand(db,f.manager,{action:"create",item:{kind:"privacy",title:"Synthetic privacy review",ownerId:f.manager.userId,data:{recordType:data.caseType,purpose:"Reviewed fixture policy; no statutory deadline is inferred"}}},key());
 await legalCommand(db,f.manager,{action:"member",id:result.id,expectedVersion:(await access(db,f.manager,result.id)).version,userId:f.tie.userId,access:"edit"},key());
 await featureCommand(db,f.manager,{action:"feature.create",id:result.id,expectedVersion:(await access(db,f.manager,result.id)).version,category:"privacy_case",title:"Synthetic privacy response",ownerId:f.manager.userId,dueDate:"2026-10-20",data:{basis:"Synthetic policy provided by the fixture",scope:"Company-only fixture data",...data}},key());
 return result.id;
}
async function row(id:string){return (await featureDetail(db,f.manager,id))[0].rows[0];}
async function command(id:string,state:string,data:Record<string,string>={},actor=f.manager,reason="Record a synthetic independently reviewable decision"){
 const current=await row(id);return featureCommand(db,actor,{action:"feature.transition",id,expectedVersion:(await access(db,actor,id)).version,category:"privacy_case",featureId:current.id,featureVersion:current.version,state,reason,data},key());
}
it("retains an earlier known breach time and requires a reviewed risk and notification decision before responding",async()=>{
 const knownAt="2026-09-29T23:30:00+07:00",id=await fixture({caseType:"breach",knownAt});
 expect(Date.parse(knownAt)).toBeLessThan((await access(db,f.manager,id)).created_at);
 const assessed={assessment:"Synthetic event risk assessment",evidence:"Synthetic event log",riskLevel:"high",notificationDecision:"pending",notificationBasis:"Awaiting the independent decision"};
 await command(id,"assessed",assessed);
 await expect(command(id,"review")).rejects.toMatchObject({code:"LEGAL_PRIVACY_REVIEW_REQUIRED"});
 await command(id,"review",{notificationDecision:"required",notificationBasis:"Reviewed fixture risk requires notification",responseDecision:"notify"});
 await expect(command(id,"approved")).rejects.toMatchObject({code:"LEGAL_INDEPENDENT_REVIEW"});
 await expect(command(id,"approved",{riskLevel:"low"},f.tie)).rejects.toMatchObject({code:"LEGAL_REVIEW_CONTENT_CHANGED"});
 await command(id,"approved",{},f.tie);await command(id,"responded",{response:"Synthetic notification delivered to the scoped recipient",evidence:"Synthetic delivery receipt"});await command(id,"closed");
 expect(await row(id)).toMatchObject({state:"closed",due_date:"2026-10-20",data:{knownAt,riskLevel:"high",notificationDecision:"required",responseDecision:"notify",evidence:"Synthetic delivery receipt"}});
 expect((await detail(db,f.manager,id)).item.due_date).toBeNull();
 const history=(await db.prepare("SELECT snapshot->'data'->>'knownAt' known_at FROM legal_feature_history WHERE item_id=? ORDER BY version").bind(id).all<{known_at:string}>()).results;
 expect(history).toHaveLength(6);expect(history.every(entry=>entry.known_at===knownAt)).toBe(true);
});
it("keeps missing identity and third-party data unresolved until the proposed DSAR response is independently reviewed",async()=>{
 const id=await fixture({caseType:"subject_request",identityVerification:"missing_information",thirdPartyData:"unknown"});
 await expect(command(id,"awaiting_information")).rejects.toMatchObject({code:"LEGAL_INVALID"});
 await command(id,"awaiting_information",{missingInformation:"Proof of authority and identity",evidence:"Clarification request receipt"});
 await command(id,"assessed",{assessment:"Synthetic identity comparison",evidence:"Received identity evidence"});
 await expect(command(id,"review",{responseDecision:"provide"})).rejects.toMatchObject({code:"LEGAL_PRIVACY_REVIEW_REQUIRED"});
 await expect(command(id,"review",{identityVerification:"verified",thirdPartyData:"present",responseDecision:"partial",refusalReason:"Synthetic third-party restriction"})).rejects.toMatchObject({code:"LEGAL_PRIVACY_REVIEW_REQUIRED"});
 await command(id,"review",{identityVerification:"verified",thirdPartyData:"present",thirdPartyHandling:"redacted",responseDecision:"partial",refusalReason:"Protect unrelated third-party data",response:"Provide only the independently reviewed redacted fixture copy"});
 await command(id,"approved",{},f.tie);await command(id,"responded",{evidence:"Redacted response delivery receipt"});await command(id,"closed");
 expect(await row(id)).toMatchObject({state:"closed",data:{thirdPartyHandling:"redacted",responseDecision:"partial",refusalReason:"Protect unrelated third-party data",missingInformation:"Proof of authority and identity"}});
 await command(id,"received");expect((await row(id)).state).toBe("received");
});
it("requires independent refusal with a retained reason and response, and preserves versioned known-time corrections",async()=>{
 const id=await fixture({caseType:"subject_request",identityVerification:"not_verified",thirdPartyData:"unknown"});
 await expect(command(id,"rejected",{evidence:"Refusal proposed"},f.tie)).rejects.toMatchObject({code:"LEGAL_PRIVACY_REVIEW_REQUIRED"});
 await command(id,"awaiting_information",{missingInformation:"Verified requester authority",evidence:"Clarification request"});
 const current=await row(id);await featureCommand(db,f.manager,{action:"feature.update",id,expectedVersion:(await access(db,f.manager,id)).version,category:"privacy_case",featureId:current.id,featureVersion:current.version,data:{responseDecision:"refuse",refusalReason:"Synthetic authority could not be verified",response:"Fictional refusal and route to resubmit",evidence:"Independent review packet"},reason:"Prepare the exact refusal for independent review"},key());
 await command(id,"rejected",{},f.tie);expect(await row(id)).toMatchObject({state:"rejected",data:{refusalReason:"Synthetic authority could not be verified",response:"Fictional refusal and route to resubmit"}});
 const breach=await fixture({caseType:"breach",knownAt:"2026-09-29T23:30:00+07:00"}),before=await row(breach),body={action:"feature.update",id:breach,expectedVersion:(await access(db,f.manager,breach)).version,category:"privacy_case",featureId:before.id,featureVersion:before.version,data:{knownAt:"2026-09-29T22:30:00+07:00"}};
 await expect(featureCommand(db,f.manager,body,key())).rejects.toMatchObject({code:"LEGAL_PRIVACY_CORRECTION_REQUIRED"});
 await featureCommand(db,f.manager,{...body,reason:"Correct the original log timestamp with retained evidence"},key());
 const snapshots=(await db.prepare("SELECT snapshot->'data'->>'knownAt' known_at FROM legal_feature_history WHERE feature_id=? ORDER BY version").bind(before.id).all<{known_at:string}>()).results;
 expect(snapshots.map(entry=>entry.known_at)).toEqual(["2026-09-29T23:30:00+07:00","2026-09-29T22:30:00+07:00"]);
});
it("refuses missing, impossible, timezone-free and future known event times before allocating a breach workflow",async()=>{
 for(const knownAt of ["","2026-02-30T12:00:00+07:00","2026-09-29T12:00:00","2999-01-01T00:00:00Z"]){await expect(fixture({caseType:"breach",knownAt})).rejects.toThrow();}
 expect(await db.prepare("SELECT count(*) n FROM legal_privacy_cases WHERE item_id IN(SELECT id FROM legal_items WHERE company_id=?)").bind(f.company).first("n")).toBe(0);
});
