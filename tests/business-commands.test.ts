import { env,applyMigrations } from "./runtime-env";
import { beforeAll,beforeEach,describe,expect,it } from "vitest";
import { assertion,executeCommand } from "../src/server/commands";
const actor={userId:"operator",assignmentId:"grant",role:"sales_operator",companyId:"company",branchId:null,departmentId:null};
describe("Business command transactions",()=>{
  beforeAll(async()=>{await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);await env.H001_DB.exec("CREATE TABLE business_fixture(id TEXT PRIMARY KEY,version BIGINT,amount BIGINT)");});
  beforeEach(async()=>{
    await env.H001_DB.exec(`
      DELETE FROM command_assertions;
      TRUNCATE command_receipts,assignments,audit_events,outbox_events,business_fixture CASCADE;
      INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('company','company','command-company','Command company','company') ON CONFLICT DO NOTHING;
      INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,parent_id,branch_id) VALUES('unrelated-branch','branch','command-branch','Other branch','company','company','unrelated-branch') ON CONFLICT DO NOTHING;
      INSERT INTO assignments(id,user_id,role,revoked_at,valid_from,valid_until,company_id,scope_kind,scope_node_id,modules_json) VALUES('grant','operator','sales_operator',NULL,0,9999999999999,'company','holding',NULL,'[]');
      INSERT INTO business_fixture VALUES('order',1,0);
    `);
  });
  function command(key:string,expectedVersion=1,amount=25){const db=env.H001_DB;return executeCommand(db,actor,key,"sales.update",{expectedVersion,amount},"order",expectedVersion+1,{id:"order",version:expectedVersion+1},[
    assertion(db,key,"version","EXISTS(SELECT 1 FROM business_fixture WHERE id='order' AND version=?)",[expectedVersion]),
    db.prepare("UPDATE business_fixture SET version=version+1,amount=amount+? WHERE id='order'").bind(amount),
  ]);}
  it("replays a committed command without repeating its effects and rejects changed input",async()=>{
    expect(await command("retry-key-1")).toEqual(await command("retry-key-1"));
    await expect(command("retry-key-1",1,30)).rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    expect(await env.H001_DB.prepare("SELECT version,amount FROM business_fixture").first()).toEqual({version:2,amount:25});
    expect(await env.H001_DB.prepare("SELECT COUNT(*) n FROM audit_events").first()).toEqual({n:1});
  });
  it("allows only one competing update and rolls back the losing receipt and evidence",async()=>{
    const results=await Promise.allSettled([command("racing-command-a"),command("racing-command-b")]);
    expect(results.filter(r=>r.status==="fulfilled")).toHaveLength(1);
    expect(await env.H001_DB.prepare("SELECT version,amount FROM business_fixture").first()).toEqual({version:2,amount:25});
    for(const table of ["command_receipts","audit_events","outbox_events"])expect(await env.H001_DB.prepare(`SELECT COUNT(*) n FROM ${table}`).first()).toEqual({n:1});
  });
  it("rejects a grant whose company or scope changed before commit",async()=>{
    await env.H001_DB.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id='unrelated-branch',branch_id='unrelated-branch' WHERE id='grant'").run();
    await expect(command("scope-changed-command")).rejects.toMatchObject({status:409});
    expect(await env.H001_DB.prepare("SELECT version,amount FROM business_fixture").first()).toEqual({version:1,amount:0});
  });
  it("rechecks revocation at commit time",async()=>{
    await env.H001_DB.prepare("UPDATE assignments SET revoked_at=1 WHERE id='grant'").run();
    await expect(command("revoked-command")).rejects.toMatchObject({status:409});
    expect(await env.H001_DB.prepare("SELECT version,amount FROM business_fixture").first()).toEqual({version:1,amount:0});
    expect(await env.H001_DB.prepare("SELECT COUNT(*) n FROM command_receipts").first()).toEqual({n:0});
  });
});
