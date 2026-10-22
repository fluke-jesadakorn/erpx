import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {pmsCommand} from "../../../packages/server-pms/src/public/pms";
import {pmsRangeReport,pmsReportEntries} from "@erpx/server-pms/public/pms/reports";
vi.mock("@erpx/server-runtime/adapters/runtime",async()=>{const {env}=await import("../../support/runtime-env");return {holdingResources:async()=>({write:env.H001_DB})};});
const db=env.H001_DB;let fixture:Awaited<ReturnType<typeof seedExpenseCompany>>,propertyId:string;
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));beforeEach(async()=>{fixture=await seedExpenseCompany(db,env.H001_FILES);propertyId=(await pmsCommand(db,fixture.owner,{operation:'property.create',input:{name:'Report hotel',branchId:fixture.branch,businessDate:'2026-09-01'}},crypto.randomUUID())).id;});
async function insert(kind:string,data:object,id=crypto.randomUUID()){await db.prepare('INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,?)').bind(id,propertyId,kind,JSON.stringify(data),Date.parse('2026-09-01T00:00:00Z')).run();return id;}
it('uses corrected room-night revenue for ADR and includes other services only in net revenue',async()=>{
 for(let i=0;i<2;i++)await insert('room',{active:true,number:String(i)});
 for(let i=0;i<2;i++)await insert('stay',{state:'checked_out',arrival:'2026-09-01',departure:'2026-09-02'});
 const first=await insert('entry',{date:'2026-09-01',kind:'charge',sourceKey:'night:room:2026-09-01',net:10000,service:0,tax:700,total:10700});
 await insert('entry',{date:'2026-09-01',kind:'adjustment',originalId:first,net:-1000,service:0,tax:-70,total:-1070});
 await insert('entry',{date:'2026-09-01',kind:'charge',sourceKey:'extra:restaurant',net:70000,service:0,tax:4900,total:74900});
 const result=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-01'});
 expect(result.totals).toMatchObject({capacity:2,sold:2,roomRevenue:9000,netRevenue:79000,tax:5530});expect(result.occupancy).toBe(1);expect(result.adr).toBe(4500);expect(result.revpar).toBe(4500);
 await insert('night',{date:'2026-09-01',state:'completed',roomMetrics:{capacity:3,sold:2}});
 expect((await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-01'})).totals.capacity).toBe(3);
});
it('bounds ranges, drill-down pages and scope',async()=>{
 for(let i=0;i<51;i++)await insert('entry',{date:'2026-09-01',kind:'receipt',net:0,service:0,tax:0,total:100,description:`Receipt ${i}`});
 const page=await pmsReportEntries(db,fixture.owner,propertyId,'2026-09-01',1);expect(page.rows).toHaveLength(50);expect(page.hasNext).toBe(true);
 expect((await pmsReportEntries(db,fixture.owner,propertyId,'2026-09-01',2)).rows).toHaveLength(1);
 await expect(pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-01-01',to:'2026-09-01'})).rejects.toThrow('Choose up to 93 days');
 await expect(pmsRangeReport(db,{...fixture.owner,companyId:'foreign'},{propertyId,from:'2026-09-01',to:'2026-09-01'})).rejects.toMatchObject({status:404});
 const cleaner=await fixture.member('reports-cleaner','pms_housekeeping');await expect(pmsReportEntries(db,cleaner,propertyId,'2026-09-01',1)).rejects.toMatchObject({status:403});
});


it('attributes current room nights by channel using original creation evidence rather than edit time',async()=>{
 const recent=Date.now()-86400000;
 const direct=await insert('reservation',{source:'direct',state:'confirmed',history:[{at:recent,actor:'test',action:'Reservation confirmed'}]});
 const legacy=await insert('reservation',{source:'ota_manual',state:'confirmed'});
 const cancelled=await insert('reservation',{source:'direct',state:'cancelled',history:[{at:recent,actor:'test',action:'Reservation confirmed'}]});
 for(const [id,state] of [[direct,'reserved'],[direct,'cancelled'],[legacy,'checked_out'],[cancelled,'cancelled']])await insert('stay',{reservationId:id,state,arrival:'2026-08-31',departure:'2026-09-03',prices:[{date:'2026-08-31',total:9000},{date:'2026-09-01',total:10000},{date:'2026-09-02',total:12000}]});
 const report=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02'});
 expect(report.channels.find(row=>row.source==='direct')).toMatchObject({reservations:2,room_nights:2,quoted_total:22000,cancelled:2,recent_pickup:2,unknown_creation:0});
 expect(report.channels.find(row=>row.source==='ota_manual')).toMatchObject({reservations:1,room_nights:2,quoted_total:22000,recent_pickup:0,unknown_creation:1});
 // A foreign company never reaches the reporting projection.
 await expect(pmsRangeReport(db,{...fixture.owner,companyId:'other'},{propertyId,from:'2026-09-01',to:'2026-09-02'})).rejects.toMatchObject({status:404});
});

it('retains historical prices and room nights after date changes and partial cancellation',async()=>{
 const reservation=await insert('reservation',{state:'confirmed',source:'direct',guestId:'private-guest',email:'private@example.test'});
 const stay=await insert('stay',{reservationId:reservation,state:'reserved',arrival:'2026-09-01',departure:'2026-09-03',guestId:'private-guest',prices:[{date:'2026-09-01',total:10000},{date:'2026-09-02',total:12000}]});
 const old=(await db.prepare('SELECT observed_at::text stamp FROM pms_booking_observations WHERE record_id=? ORDER BY observation_id DESC LIMIT 1').bind(stay).first<{stamp:string}>())!;
 const change={reservationId:reservation,state:'reserved',arrival:'2026-09-02',departure:'2026-09-03',prices:[{date:'2026-09-02',total:15000}]};
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare('UPDATE pms_records SET data_json=?,version=version+1 WHERE id=?').bind(JSON.stringify(change),stay).run();});
 const report=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02',compareAsOf:new Date(old.stamp).toISOString()});
 // Preserve PostgreSQL's microsecond observation boundary rather than rounding down.
 const exact=old.stamp.replace(' ','T').replace('+00','Z');
 const retained=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02',compareAsOf:exact});
 expect(retained.pace.previous).toMatchObject({covered:true,room_nights:2,quoted_total:22000});
 expect(retained.pace.current).toMatchObject({covered:true,room_nights:1,quoted_total:15000});
 expect(retained.pace.netPickup).toMatchObject({roomNights:-1,quotedTotal:-7000});
 expect(report.pace.current?.room_nights).toBe(1);
 const facts=(await db.prepare('SELECT facts::text facts FROM pms_booking_observations WHERE property_id=?').bind(propertyId).all<{facts:string}>()).results;
 expect(JSON.stringify(facts)).not.toContain('private-guest');expect(JSON.stringify(facts)).not.toContain('private@example');
 await expect(db.prepare("UPDATE pms_booking_observations SET evidence='baseline' WHERE record_id=?").bind(stay).run()).rejects.toThrow('PMS_BOOKING_OBSERVATION_IMMUTABLE');
 const before=(await db.prepare('SELECT count(*)::integer count FROM pms_booking_observations WHERE property_id=?').bind(propertyId).first<{count:number}>())!.count;
 await expect(db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare('UPDATE pms_records SET data_json=?,version=version+1 WHERE id=?').bind(JSON.stringify({...change,state:'cancelled'}),stay).run();throw new Error('ROLLBACK_REHEARSAL');})).rejects.toThrow('ROLLBACK_REHEARSAL');
 expect((await db.prepare('SELECT count(*)::integer count FROM pms_booking_observations WHERE property_id=?').bind(propertyId).first<{count:number}>())!.count).toBe(before);
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare('UPDATE pms_records SET data_json=?,version=version+1 WHERE id=?').bind(JSON.stringify({...change,state:'cancelled'}),stay).run();});
 const cancelled=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02',compareAsOf:exact});
 expect(cancelled.pace.previous?.quoted_total).toBe(22000);expect(cancelled.pace.netPickup).toMatchObject({roomNights:-2,quotedTotal:-22000});
});
it('reports unknown pace before coverage instead of inventing legacy observations',async()=>{
 const report=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02',compareAsOf:'2020-01-01T00:00:00Z'});
 expect(report.pace.previous?.covered).toBe(false);expect(report.pace.netPickup).toBeNull();
 await expect(pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02',asOf:'2099-01-01T00:00:00Z'})).rejects.toMatchObject({code:'PMS_PACE_RANGE'});
});

it('retains completed reservation room nights in the historical booked stay period',async()=>{
 const r=await insert('reservation',{state:'completed',source:'phone'});await insert('stay',{reservationId:r,state:'checked_out',arrival:'2026-09-01',departure:'2026-09-03',prices:[{date:'2026-09-01',total:10000},{date:'2026-09-02',total:12000}]});
 const report=await pmsRangeReport(db,fixture.owner,{propertyId,from:'2026-09-01',to:'2026-09-02'});expect(report.pace.current).toMatchObject({room_nights:2,quoted_total:22000});
});
