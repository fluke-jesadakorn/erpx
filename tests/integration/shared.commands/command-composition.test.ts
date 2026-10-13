import { beforeAll, beforeEach, expect, it } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { assertion, executeCommand } from "../../../src/server/modules/workflow/public/commands";

const db = env.H001_DB;
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => {
  await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);
  await db.exec("CREATE TABLE engineering_command_probe(id TEXT PRIMARY KEY,value INTEGER NOT NULL)");
});
beforeEach(async () => { f = await seedExpenseCompany(db,env.H001_FILES); });

it("rolls back a composed handoff and preserves its original constraint cause", async () => {
  const key = crypto.randomUUID();
  await expect(db.transaction(async tx => {
    await tx.prepare("INSERT INTO engineering_command_probe VALUES(?,1)").bind(key).run();
    return executeCommand(tx,f.owner,key,"engineering.probe",{value:2},key,1,{id:key},[
      tx.prepare("UPDATE engineering_command_probe SET value=2 WHERE id=?").bind(key),
      assertion(tx,key,"stale-handoff","false"),
    ]);
  })).rejects.toMatchObject({code:"CONFLICT",cause:{code:"23514"}});
  expect(await db.prepare("SELECT id FROM engineering_command_probe WHERE id=?").bind(key).first()).toBeNull();
  for (const table of ["command_receipts","audit_events","outbox_events"])
    expect(await db.prepare(`SELECT COUNT(*)::int n FROM ${table} WHERE command_id=?`).bind(key).first()).toEqual({n:0});
});

it("lets the owning transaction retry serialization failures across the whole workflow", async () => {
  const key = crypto.randomUUID();
  let attempts = 0;
  await expect(db.transaction(async tx => {
    attempts++;
    await tx.prepare("INSERT INTO engineering_command_probe VALUES(?,1)").bind(key).run();
    return executeCommand(tx,f.owner,key,"engineering.probe",{},key,1,{id:key},[
      tx.prepare("DO $$ BEGIN RAISE EXCEPTION 'forced serialization fixture' USING ERRCODE='40001'; END $$"),
    ]);
  })).rejects.toMatchObject({code:"40001"});
  expect(attempts).toBe(4);
  expect(await db.prepare("SELECT id FROM engineering_command_probe WHERE id=?").bind(key).first()).toBeNull();
  expect(await db.prepare("SELECT command_id FROM command_receipts WHERE command_id=?").bind(key).first()).toBeNull();
});

it("recovers a committed composed command after a lost response without repeating its handoff", async () => {
  const key = crypto.randomUUID(), input = {value:2};
  const command = () => db.transaction(tx => executeCommand(tx,f.owner,key,"engineering.probe",input,key,1,{id:key,value:2},[
    tx.prepare("INSERT INTO engineering_command_probe VALUES(?,2)").bind(key),
  ]));
  expect(await command()).toEqual({id:key,value:2});
  expect(await command()).toEqual({id:key,value:2});
  expect(await db.prepare("SELECT COUNT(*)::int n FROM engineering_command_probe WHERE id=?").bind(key).first()).toEqual({n:1});
  expect(await db.prepare("SELECT COUNT(*)::int n FROM outbox_events WHERE command_id=?").bind(key).first()).toEqual({n:1});
});
