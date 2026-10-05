import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {seedExpenseCompany} from '../fixtures/expenses/company';
import {pmsCommand} from '../src/server/pms';
import {pmsRangeReport,pmsReportEntries} from '../src/server/pms/reports';
vi.mock('../src/server/runtime',async()=>{const {env}=await import('./runtime-env');return {holdingResources:async()=>({write:env.H001_DB})};});
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
