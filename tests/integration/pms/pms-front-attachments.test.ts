import {afterAll,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {pmsCommand} from "../../../src/server/modules/pms/public/pms";
import {frontPmsAttachmentRequest} from "../../../src/server/modules/pms/public/pms/front-attachments";
import {scanAttachmentNow} from "../../../src/server/modules/files/public/attachments/service";
import {attachmentApiBase} from "../../../src/lib/files/attachment-client";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",async()=>{const {env}=await import("../../support/runtime-env");return {holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES})};});
// This adapter/security test uses a scanner double; real ClamAV has separate acceptance.
vi.mock("../../../src/server/modules/files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Scanner double',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Scanner double',definitionUpdatedAt:Date.now()})}));
const db=env.H001_DB;
let fixture:Awaited<ReturnType<typeof seedExpenseCompany>>,propertyId:string,taskId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2='true';});
afterAll(()=>{delete process.env.ERPX_ATTACHMENTS_V2;});
beforeEach(async()=>{
 fixture=await seedExpenseCompany(db,env.H001_FILES);
 propertyId=(await pmsCommand(db,fixture.owner,{operation:'property.create',input:{name:'Front file fixture',branchId:fixture.branch,businessDate:'2026-09-01'}},crypto.randomUUID())).id;
 const typeId=crypto.randomUUID(),roomId=crypto.randomUUID();
 await db.batch([
  db.prepare('UPDATE pms_properties SET state=\'open\' WHERE id=?').bind(propertyId),
  db.prepare('INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,\'room_type\',?,1,1)').bind(typeId,propertyId,JSON.stringify({name:'Standard',capacity:2,amenities:''})),
  db.prepare('INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,\'room\',?,1,1)').bind(roomId,propertyId,JSON.stringify({number:'101',typeId,building:'A',floor:'1',active:true,cleanliness:'dirty',cleaningCycle:1,dnd:false,observedOccupied:false})),
 ]);
 taskId=(await pmsCommand(db,fixture.owner,{operation:'task.create',input:{roomId,type:'arrival',date:'2026-09-01'},propertyId,expectedVersion:1},crypto.randomUUID())).id;
});
function request(path:string,method='GET',body?:unknown){return new Request(`http://127.0.0.1:3104/api/v1/front/acme/hotel/pms/${path}`,{method,headers:{host:'127.0.0.1:3104',origin:'http://127.0.0.1:3104','Content-Type':'application/json','Idempotency-Key':crypto.randomUUID()},...(body===undefined?{}:{body:JSON.stringify(body)})});}
it('keeps private room evidence on Front through intake, scan and the hotel action binding',async()=>{
 expect(attachmentApiBase('/front/acme/hotel/pms')).toBe('/api/v1/front/acme/hotel/pms');
 expect(()=>attachmentApiBase('/front/acme/hotel/pos')).toThrow();
 const input={targetType:'pms_task',targetId:taskId,componentKind:'action',componentRef:'task.complete',purposeKey:'room_photo',expectedTargetVersion:1};
 const set=(await (await frontPmsAttachmentRequest(request('attachment-sets','POST',input),fixture.owner,['attachment-sets'])).json()).data;
 const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
 const file=(await (await frontPmsAttachmentRequest(request(`attachment-sets/${set.id}/files`,'POST',{filename:'room.png',mediaType:'image/png',bytes:png.length}),fixture.owner,['attachment-sets',set.id,'files'])).json()).data;
 expect(file.uploadUrl).toMatch(/^\/api\/v1\/front\/acme\/hotel\/pms\/uploads\/[^/]+\/content$/);
 const upload=new Request(`http://127.0.0.1:3104${file.uploadUrl}`,{method:'PUT',headers:{host:'127.0.0.1:3104',origin:'http://127.0.0.1:3104','Content-Type':'image/png','Content-Length':String(png.length)},body:png});
 const saved=(await (await frontPmsAttachmentRequest(upload,fixture.owner,['uploads',file.uploadIntentId,'content'])).json()).data;
 await scanAttachmentNow(db,env.H001_FILES,saved.sourceId);
 const row=await db.prepare('SELECT data_json FROM pms_records WHERE id=?').bind(taskId).first<{data_json:string}>();
 await pmsCommand(db,fixture.owner,{operation:'task.complete',propertyId,expectedVersion:2,input:{id:taskId,checked:JSON.parse(row!.data_json).checklist,notes:'Fictional test evidence'},attachmentSetIds:[set.id]},crypto.randomUUID());
 const status=(await (await frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),fixture.owner,['attachment-links',file.attachmentLinkId,'status'])).json()).data;
 expect(status.linkState).toBe('bound');
 const other=await seedExpenseCompany(db,env.H001_FILES);
 await expect(frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),other.owner,['attachment-links',file.attachmentLinkId,'status'])).rejects.toMatchObject({status:404});
 await expect(frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),{...fixture.owner,branchId:'other-branch'},['attachment-links',file.attachmentLinkId,'status'])).rejects.toMatchObject({status:403});
});
it('rejects non-hotel targets, publication and administrative actions on Front',async()=>{
 await expect(frontPmsAttachmentRequest(request('attachment-sets','POST',{targetType:'payroll_run',targetId:taskId,componentKind:'record',purposeKey:'room_photo',expectedTargetVersion:1}),fixture.owner,['attachment-sets'])).rejects.toThrow();
 await expect(frontPmsAttachmentRequest(request('media-publications','POST',{}),fixture.owner,['media-publications'])).rejects.toMatchObject({status:404});
 await expect(frontPmsAttachmentRequest(request('attachment-sets','POST',{targetType:'pms_task',targetId:taskId,componentKind:'publication',purposeKey:'room_photo',expectedTargetVersion:1}),fixture.owner,['attachment-sets'])).rejects.toThrow();
});
