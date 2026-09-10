import { env } from "./runtime-env";
import { beforeEach,describe,expect,it } from "vitest";
import { project,type EventMessage } from "../src/workers/jobs";
describe("Workflow event projection",()=>{
  beforeEach(async()=>{
    await env.H001_READ_DB.exec(`
      DROP TABLE IF EXISTS work_items;
      DROP TABLE IF EXISTS projection_receipts;
      DROP TABLE IF EXISTS projection_checkpoints;
      CREATE TABLE work_items(id TEXT PRIMARY KEY,source_type TEXT,source_id TEXT,source_version BIGINT,title TEXT,subtitle TEXT,queue TEXT,company_id TEXT,branch_id TEXT,department_id TEXT,updated_at BIGINT,UNIQUE(source_type,source_id));
      CREATE TABLE projection_receipts(event_id TEXT PRIMARY KEY,aggregate_id TEXT,aggregate_version BIGINT,applied_at BIGINT);
      CREATE TABLE projection_checkpoints(projector TEXT PRIMARY KEY,last_event_id TEXT,last_event_at BIGINT,updated_at BIGINT);
    `);
    await env.H001_DB.exec("DROP TABLE IF EXISTS hr_payroll_runs; CREATE TABLE hr_payroll_runs(id TEXT PRIMARY KEY,status TEXT,version BIGINT,company_id TEXT); INSERT INTO hr_payroll_runs VALUES('run','approved',5,'company');");
  });
  const message=(id:string,version:number,state?:string):EventMessage=>({holdingId:"h001",id,eventType:"people.payroll.calculate",aggregateId:"run",aggregateVersion:version,payload:{id:"run",state,version,companyId:"company",branchId:null,departmentId:null}});
  it("recovers older events missing state from the owner and rejects stale projection updates",async()=>{
    await project(env,message("old-format",2));
    await project(env,message("delayed-preview",3,"preview"));
    await project(env,message("old-format",2));
    expect(await env.H001_READ_DB.prepare("SELECT source_version,subtitle,queue FROM work_items").first()).toEqual({source_version:5,subtitle:"approved",queue:"completed"});
    expect(await env.H001_READ_DB.prepare("SELECT COUNT(*) n FROM projection_receipts").first()).toEqual({n:2});
  });
  it("uses the current task and its scope for duplicate or delayed approval and generated events",async()=>{
    await env.H001_DB.exec("DROP TABLE IF EXISTS staff_tasks; CREATE TABLE staff_tasks(id TEXT PRIMARY KEY,state TEXT,version BIGINT,company_id TEXT,branch_id TEXT,department_id TEXT); INSERT INTO staff_tasks VALUES('task','completed',8,'company','record-branch','record-department');");
    const event:EventMessage={...message("task-generated",1,"ready"),eventType:"task.generated",aggregateId:"task"};
    await project(env,event);await project(env,event);await project(env,{...event,id:"old-reroute",eventType:"task.approval.reroute",aggregateVersion:4});
    expect(await env.H001_READ_DB.prepare("SELECT source_version,subtitle,queue,branch_id,department_id FROM work_items").first()).toEqual({source_version:8,subtitle:"completed",queue:"completed",branch_id:"record-branch",department_id:"record-department"});
    expect(await env.H001_READ_DB.prepare("SELECT COUNT(*) n FROM projection_receipts").first()).toEqual({n:2});
  });
  it("closes cancelled leave work items",async()=>{
    const event={...message("leave-cancel",4,"cancelled"),eventType:"leave.transition",aggregateId:"leave"};
    await project(env,event);
    expect(await env.H001_READ_DB.prepare("SELECT queue,subtitle FROM work_items").first()).toEqual({queue:"completed",subtitle:"cancelled"});
  });
});
