import {architectureStatus} from "../../../scripts/architecture/status";
import { describe, expect, test } from "vitest";
import { inspectSource, evaluateBaseline } from "../../../scripts/architecture/boundaries";
import { moduleIds, ownersForSource, ownershipPatternMatches, serviceOwnership } from "@erpx/contracts/ownership";
import registry from "@erpx/catalogue/module-registry.json";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {inspectArchitecture} from "../../../scripts/architecture/boundaries";
import {runArchitectureAcceptance} from "../../../scripts/architecture/acceptance";
import {spawnSync} from "node:child_process";
import {pinnedBunBinary} from "../../../scripts/testing/runtime-tooling";

const root = process.cwd();
const domain = "packages/server-brand/src/domain/policy.ts";
const application = "packages/server-brand/src/application/publish.ts";
const rules = (source: string, text: string) => inspectSource(root, source, text).map((issue) => issue.rule);

describe("architecture enforcement", () => {
  test("owns exactly the canonical module registry and every legacy server source has one owner", () => {
    expect([...moduleIds].sort()).toEqual(registry.map((module) => module.id).sort());
    expect(serviceOwnership.owners.filter((owner) => owner.kind === "business").map((owner) => owner.id).sort()).toEqual([...moduleIds].sort());
    for (const owner of serviceOwnership.owners) {
      for (const source of owner.source.filter((source) => !source.includes("*"))) expect(ownersForSource(source).map((entry) => entry.id)).toEqual([owner.id]);
      if(owner.kind==="business")expect(fs.existsSync(path.join(root,"packages",`server-${owner.id}`,"src/index.ts"))).toBe(true);
      for(const capability of owner.capabilities??[]){
        expect(["legacy","partial","clean"]).toContain(capability.migration);
        expect(capability.regression.length).toBeGreaterThan(0);
        for(const file of [...capability.source,...capability.regression])expect(fs.existsSync(path.join(root,file))).toBe(true);
        if(capability.migration==="clean")expect(capability.source.some(file=>/\/(domain|application)\//.test(file))).toBe(true);
      }
    }
  });
  test("matches source patterns without crossing a slash for a single star", () => {
    expect(ownershipPatternMatches("packages/server-brand/src/**", "./packages/server-brand/src/domain/policy.ts")).toBe(true);
    expect(ownershipPatternMatches("src/server/*.ts", "src/server/brand.ts")).toBe(true);
    expect(ownershipPatternMatches("src/server/*.ts", "src/server/brand/service.ts")).toBe(false);
    expect(ownershipPatternMatches("src/server/a.ts", "src/server/aXts")).toBe(false);
  });
  test("owns portable rules and rejects private helper imports across owners", () => {
    for (const [owner, helper, facade] of [
      ["organization", "packages/feature-organization/src/lib/organization-domain.ts", "organization-domain"],
      ["people_payroll", "packages/feature-people_payroll/src/lib/people-roster-domain.ts", "people-roster-domain"],
      ["work_projection", "packages/feature-work_projection/src/lib/calendar-domain.ts", "calendar-domain"],
      ["work_projection", "packages/feature-work_projection/src/lib/calendar-date-domain.ts", "calendar-date-domain"],
      ["runtime", "packages/feature-runtime/src/lib/executive/canvas-domain.ts", "canvas-domain"],
    ]) {
      expect(ownersForSource(helper).map((entry) => entry.id)).toEqual([owner]);
      expect(rules(application, `import {rule} from "${helper.replace(/^packages\/([^/]+)\/src\/(.+)\.ts$/, "@erpx/$1/$2")}";`)).toContain("owner-private-import");
      expect(rules(application, `import {rule} from "@erpx/server-${owner}/public/rules/${facade}";`)).toEqual([]);
    }
  });
  test("rejects SQL and persistence types inside a core regardless of import style", () => {
    expect(rules(domain, 'import type { SqlDatabase as Database } from "@/server/runtime/types"; const text=`SELECT * FROM records`;')).toContain("core-persistence-handle");
    expect(rules(domain, 'import type { SqlDatabase as Database } from "@/server/runtime/types"; const text=`SELECT * FROM records`;')).toContain("core-sql");
    expect(rules(application, 'const infrastructure = await import("@/server/runtime");')).toContain("core-legacy-service");
    expect(rules(application, 'const infrastructure = require("pg");')).toContain("core-infrastructure");
    expect(rules(domain, 'throw new Error("Select the billing month.");')).toEqual([]);
    expect(rules(domain, 'const query="SELECT 1";')).toContain("core-sql");
    expect(rules(domain, 'const message=`Update from our team: ${note}`;')).toEqual([]);
    expect(rules(domain, 'const query="UPDATE users SET name=?";')).toContain("core-sql");
    expect(rules(domain, 'const query=\'UPDATE "from" SET name=?\';')).toContain("core-sql");
  });
  test("rejects Next, global environment, clocks and transport inside a core", () => {
    const found = rules(application, 'import {headers} from "next/headers"; const now=Date.now(); const value=process.env.VALUE; fetch("/api");');
    expect(found).toContain("core-infrastructure");
    expect(found).toContain("core-global-environment");
    expect(found.filter((rule) => rule === "core-global-effect")).toHaveLength(2);
  });
  test("rejects ambient clock constructors and random calls by AST syntax in both core layers", () => {
    for (const source of [domain, application]) for (const code of [
      "Date.now()", "Date . now ()", 'Date["now"]()', "globalThis.Date.now()",
      "new Date()", "new Date", "new (Date)()", "new globalThis.Date()",
      "Math.random()", 'Math["random"]()', "globalThis.Math.random()",
      "crypto.randomUUID()", 'globalThis.crypto["randomUUID"]()',
    ]) {
      const found = inspectSource(root, source, `const value=${code};`);
      expect(found).toHaveLength(1);
      expect(found[0].rule).toBe("core-global-effect");
      expect(found[0].grandfatherable).toBe(false);
    }
  });
  test("allows deterministic Date inputs, injected effects, and native adapter clocks", () => {
    const code = `
      const date=new Date(ports.clock.now());
      const fixed=new Date("2026-10-06T00:00:00Z");
      const epoch=new globalThis.Date(0);
      const id=ports.identifiers.next();
      const random=ports.random.next();
      const custom=ports.Date.now();
      const constructor=new ports.Date();
      const description="new Date() and Date.now()";
      // Math.random() is described here, not executed.
    `;
    expect(rules(domain, code)).toEqual([]);
    expect(rules(application, code)).toEqual([]);
    expect(rules("packages/server-brand/src/adapters/clock.ts", "const now=new Date(); const id=crypto.randomUUID(); const value=Date.now()+Math.random();")).toEqual([]);
  });
  test("rejects literal SQL execution in UI API and worker entrypoints", () => {
    for (const source of ["src/app/finance/page.tsx", "src/app/api/finance/route.ts", "src/workers/finance.ts", "src/components/finance.tsx"]) for (const code of [
      'write.prepare("SELECT * FROM finance_periods").bind(companyId);',
      'write.pool.query("UPDATE finance_periods SET state=$1 WHERE id=$2", values);',
      'pool.query(`SELECT ${fields} FROM ${relation} WHERE company_id=$1`, values);',
      'db.prepare(("SELECT " + fields + " FROM finance_periods WHERE company_id=?"));',
    ]) {
      const found = inspectSource(root, source, code).filter(issue => issue.rule === "entrypoint-sql");
      expect(found).toHaveLength(1);
      expect(found[0].grandfatherable).toBe(false);
    }
  });
  test("rejects cooked SQL passed through any callable argument in presentation API and workers", () => {
    for (const source of ["src/app/pos/page.tsx", "src/app/api/pos/route.ts", "src/workers/pos.ts", "src/components/pos.tsx"]) for (const code of [
      'one(db,"SELECT sale_id FROM pos_lines",saleId);',
      'import {one as lookup} from "@erpx/server-pos/public/pos-core"; lookup(db,`SELECT ${fields} FROM pos_lines WHERE id=?`,id);',
      'import * as queries from "@erpx/server-pos/public/pos-core"; queries.all(db,"SELECT sale_id FROM pos_lines");',
      'const queries=await import("@erpx/server-pos/public/pos-core"); queries.one(db,"SELECT sale_id FROM pos_lines");',
      'const {one:lookup}=require("@erpx/server-pos/public/pos-core"); lookup(db,"SELECT sale_id FROM pos_lines");',
      'const prefix="SELECT sale_id "; const query=prefix+"FROM pos_lines"; lookup(db,query);',
      'const prefix="SELECT"; const relation="pos_lines"; lookup(db,`${prefix} sale_id FROM ${relation}`);',
      'const query="SELECT sale_id FROM pos_lines"; const copied=query; arbitrary(actor,{},copied);',
      'lookup(db,{sql:"SELECT sale_id FROM pos_lines",args:[id]});',
      'lookup(db,["SELECT sale_id FROM pos_lines"]);',
      'lookup(db,condition?"SELECT sale_id FROM pos_lines":"SELECT id FROM pos_sales");',
      'new Query(db,"SELECT sale_id FROM pos_lines");',
      'queries.sql`SELECT ${fields} FROM pos_lines`;',
    ]) {
      const found = inspectSource(root, source, code).filter(issue => issue.rule === "entrypoint-sql");
      expect(found, `${source}: ${code}`).toHaveLength(1);
      expect(found[0].grandfatherable).toBe(false);
    }
  });
  test("resolves immutable SQL constants without leaking across nested scopes or shadowed parameters", () => {
    const source = "src/app/api/pos/route.ts";
    for (const code of [
      'const query="SELECT id FROM pos_sales"; function read(){ lookup(db,query); }',
      'const start="SELECT "; function read(){ const query=start+"id FROM pos_sales"; lookup(db,query); }',
      'const query="SELECT id FROM pos_sales"; { const copied=(query as string); lookup(db,copied!); }',
    ]) expect(rules(source, code).filter(rule => rule === "entrypoint-sql")).toHaveLength(1);
    for (const code of [
      'const query="SELECT id FROM pos_sales"; function display(query:string){ owner.query(query); }',
      'const query="SELECT id FROM pos_sales"; { const query="Select a sale."; owner.query(query); }',
      'function example(){ const query="SELECT id FROM pos_sales"; } function display(){ owner.query(query); }',
      'const a=b; const b=a; owner.query(a);',
    ]) expect(rules(source, code)).toEqual([]);
  });
  test("keeps owner semantic arguments UI instructions and structured business errors outside the SQL guard", () => {
    const source = "src/app/pos/page.tsx";
    for (const code of [
      'import {holdingBindingBySlug,isHoldingId} from "@erpx/server-runtime/public/runtime"; const binding=await holdingBindingBySlug(slug); isHoldingId(binding.id);',
      'import {getExpense} from "@erpx/server-people_payroll/public/expenses"; getExpense(actor,id);',
      'owner.query({companyId,filters:{state:"posted"}}); owner.prepare({periodId});',
      'showMessage("Select a current delivery from this checkout.");',
      'fail(409,"DELIVERY_REQUIRED","Select a current delivery from this checkout.");',
      'ports.error({status:409,code:"DELIVERY_REQUIRED",message:"Select a current delivery from this checkout."});',
      'const sample="SELECT sale_id FROM pos_lines"; const note=<p>SELECT sale_id FROM pos_lines</p>;',
    ]) expect(rules(source, code)).toEqual([]);
    expect(rules(source, 'fail(409,"DELIVERY_REQUIRED","SELECT sale_id FROM pos_lines");')).toContain("entrypoint-sql");
    expect(rules(source, 'ports.error({status:409,code:"DELIVERY_REQUIRED",message:"SELECT sale_id FROM pos_lines"});')).toContain("entrypoint-sql");
    expect(rules(application, 'const query="Select a current delivery from this checkout.";')).toContain("core-sql");
  });
  test("rejects native resource acquisition and bindings in non-API presentation through every import form", () => {
    for (const source of ["src/app/holdings/[holdingSlug]/page.tsx", "src/app/book/page.tsx", "src/app/pos/page.ts", "src/app/pos/presentation-helper.ts", "src/app/api/preview/page.tsx", "src/components/booking.tsx"]) for (const code of [
      'import {holdingResources as resources} from "@erpx/server-runtime/public/runtime"; resources(holdingId);',
      'import * as runtime from "@erpx/server-runtime/public/runtime"; runtime.holdingResources(holdingId);',
      'const runtime=await import("@erpx/server-runtime/public/runtime"); runtime.holdingResources(holdingId);',
      'const runtime=require("@erpx/server-runtime/public/runtime/env"); runtime.getRuntime();',
      'import runtime = require("@erpx/server-runtime/public/runtime"); runtime.holdingResources(holdingId);',
      'import {iamContext as context} from "@erpx/server-identity_access/public/iam-context"; const {db}=await context(slug);',
      'import * as iam from "@erpx/server-identity_access/public/iam-context"; iam.iamContext(slug);',
      'const {iamContext:context}=await import("@erpx/server-identity_access/public/iam-context"); context(slug);',
      'const {bookingDatabase:database}=require("@erpx/server-pms/public/pms-booking"); database(slug);',
      'const resource=holdingResources; const copied=resource; copied(holdingId);',
      'const key="bookingDatabase"; booking[key](slug);',
      'const db=env.CONTROL_DB; owner.query(db,actor);',
      'const db=env["CONTROL_READ_DB"]; owner.query(db,actor);',
      'const key="H001_DB"; owner.query(env[key],actor);',
      'const key="CONTROL_DB"; const copied=key; owner.query(env[copied],actor);',
      'const {CONTROL_DB:db}=env; owner.query(db,actor);',
      'const {H007_READ_DB}=env; owner.query(H007_READ_DB,actor);',
      'const files=env.H001_FILES; files.get(id);',
      'import {staffPage as context} from "@erpx/server-work_projection/public/staff-page"; const {write}=await context(slug);',
      'import {portalContext as context} from "@erpx/server-legal/public/legal/portal"; const {write}=await context(slug);',
      'const {customerPortalContext:context}=await import("@erpx/server-customer_authority/public/customer-portal"); const {files}=await context(slug);',
      'import {Pool} from "pg"; const db=new Pool();',
    ]) {
      const found = inspectSource(root, source, code).filter(issue => issue.rule.endsWith("runtime-composition"));
      expect(found, `${source}: ${code}`).toHaveLength(1);
      expect(found[0].grandfatherable).toBe(false);
    }
  });
  test("rejects generic SQL resource facades in presentation while allowing type-only contracts", () => {
    const source = "src/app/pos/page.tsx";
    for (const code of [
      'import {one as read} from "@erpx/server-pos/public/pos-core"; read(db,query);',
      'import * as queries from "@erpx/server-pos/public/pos-core"; const read=queries.one; read(db,query);',
      'const queries=await import("@erpx/server-pos/public/pos-core"); queries.all(db,query);',
      'const modulePath="@erpx/server-pos/public/pos-core"; const {one:read}=require(modulePath); read(db,query);',
      'import queries = require("@erpx/server-pos/public/pos-core"); queries.one(db,query);',
      'const {run:write}=require("@erpx/server-work_projection/public/calendar/access"); write(db,query);',
      'import {readListPage as read} from "@erpx/server-work_projection/public/list-page"; read(db,{sql:query,args:[]});',
      'import {assertion as guard} from "@erpx/server-workflow/public/commands"; guard(db,key,value,predicate,bindings);',
    ]) expect(rules(source, code)).toContain("presentation-runtime-composition");
    expect(rules(source, 'import type {RuntimeEnv} from "@erpx/server-runtime/public/runtime/env"; import {type PosRow} from "@erpx/server-pos/public/pos-core";')).toEqual([]);
    expect(rules(source, 'import {fail,str} from "@erpx/server-pos/public/pos-core"; str(input); fail(409,"POS_CHANGED","Review this sale.");')).toEqual([]);
    expect(rules(source, 'import * as queries from "@erpx/server-pos/public/pos-core"; queries.str(input); function view(queries:{one(value:{id:string}):unknown}){return queries.one({id:"a"});}')).toEqual([]);
    expect(rules(source, 'import * as queries from "@erpx/server-pos/public/pos-core"; {const queries=ports; queries.one({id:"a"});}')).toEqual([]);
    expect(rules(source, 'import * as queries from "@erpx/server-pos/public/pos-core"; function view(){if(flag){var queries=ports;} return queries.one({id:"a"});}')).toEqual([]);
    expect(rules(source, 'import * as queries from "@erpx/server-pos/public/pos-core"; function view(){function queries(){} return queries.one({id:"a"});}')).toEqual([]);
    expect(rules(source, 'import * as queries from "@erpx/server-pos/public/pos-core"; function view(queries:{one(value:{id:string}):unknown}){return queries.one({id:"a"});} queries.one(db,query);')).toContain("presentation-runtime-composition");
  });
  test("retains typed resource composition in HTTP and worker adapters without admitting their SQL", () => {
    for (const source of ["src/app/api/pos/route.ts", "src/app/book/[holdingSlug]/route.ts", "src/workers/pos.ts", "packages/server-pos/src/adapters/presentation-queries.ts"]) {
      const code='import {runtimeEnv,holdingResources} from "@erpx/server-runtime/public/runtime"; const env=await runtimeEnv(); const db=env.CONTROL_DB; const {write}=await holdingResources(holdingId); owner.query(db,actor);';
      expect(rules(source, code)).toEqual([]);
      if (!source.includes("/adapters/")) expect(rules(source, `${code} arbitrary(db,"SELECT id FROM pos_sales");`)).toContain("entrypoint-sql");
      else expect(rules(source, `${code} arbitrary(db,"SELECT id FROM pos_sales");`)).toEqual([]);
    }
  });
  test("allows public owner queries/preparation and adapter SQL while ignoring UI prose", () => {
    const source = "src/app/finance/page.tsx";
    expect(rules(source, 'const note="SELECT id FROM finance_periods"; const message="Select the current period.";')).toEqual([]);
    expect(rules(source, 'owner.prepare({companyId,periodId}); owner.query({periodId}); owner.prepare("Select the current period.");')).toEqual([]);
    expect(rules(source, 'periodsForCompany(write, companyId); prepareInvoice(actor, input);')).toEqual([]);
    expect(rules("packages/server-finance/src/adapters/period-queries.ts", 'db.prepare("SELECT * FROM finance_periods"); db.pool.query("SELECT id FROM finance_periods");')).toEqual([]);
    expect(rules("packages/feature-client/src/components/finance.tsx", '"use client"; const text="SELECT id FROM finance_periods"; owner.prepare({periodId});')).toEqual([]);
    expect(rules("src/components/finance.tsx", 'import {getExpense} from "@erpx/server-people_payroll/public/expenses"; getExpense(actor,id);')).toEqual([]);
  });
  test("rejects native runtime composition in components through aliases and dynamic imports", () => {
    const source = "packages/feature-workflow/src/components/approval-administration.tsx";
    for (const code of [
      'import {holdingResources as resources} from "@erpx/server-runtime/public/runtime"; resources(holdingId);',
      'import * as runtime from "@erpx/server-runtime/public/runtime"; runtime.holdingResources(holdingId);',
      'const runtime=await import("@/server/runtime"); runtime.holdingResources(holdingId);',
      'const runtime=require("@erpx/server-runtime/public/runtime/env"); runtime.getRuntime();',
      'import {iamDb as database} from "@erpx/server-identity_access/public/iam-db"; database(write);',
      'holdingResources(holdingId);', 'runtimeEnv();', 'controlResources();', 'iamDb(write);',
    ]) {
      const found = inspectSource(root, source, code).filter(issue => issue.rule === "component-runtime-composition");
      expect(found).toHaveLength(1);
      expect(found[0].grandfatherable).toBe(false);
    }
    expect(rules(source, 'import type {RuntimeEnv} from "@erpx/server-runtime/public/runtime/env";')).toEqual([]);
    expect(rules(source, 'import {type HoldingId} from "@erpx/server-runtime/public/runtime";')).toEqual([]);
    expect(rules(source, 'const description="holdingResources runtimeEnv controlResources iamDb"; owner.query({companyId});')).toEqual([]);
    expect(rules(source, 'import {currentSession} from "@erpx/server-identity_access/public/authorization"; currentSession();')).toEqual([]);
  });
  test("rejects cooked projection placeholders only when SQL is prepared or queried", () => {
    const source = "packages/server-customer_authority/src/adapters/portal-access.ts";
    for (const code of [
      'db.prepare(`SELECT * FROM (\\${customer_360ProjectionSources.customer_portal_actions}) actions WHERE user_id=?`);',
      'db.pool.query("SELECT * FROM (${customer360ProjectionSources.customer_portal_actions}) actions WHERE user_id=$1", [userId]);',
      'db.prepare(`SELECT ${fields} FROM (\\${customer360ProjectionSources.customer_portal_actions}) actions WHERE user_id=?`);',
      'db.prepare(("SELECT * FROM (" + "${customer360ProjectionSources.customer_portal_actions}" + ") actions"));',
    ]) {
      const found = inspectSource(root, source, code);
      expect(found).toHaveLength(1);
      expect(found[0].rule).toBe("sql-unexpanded-projection");
      expect(found[0].target).toMatch(/^\$\{customer_?360ProjectionSources\.customer_portal_actions\}$/);
      expect(found[0].grandfatherable).toBe(false);
    }
    expect(rules(source, 'db.prepare(`SELECT * FROM (${customer360ProjectionSources.customer_portal_actions}) actions WHERE user_id=?`).bind(userId);')).toEqual([]);
    expect(rules(source, 'db.pool.query(`SELECT ${fields} FROM (${customer360ProjectionSources.customer_portal_actions}) actions WHERE user_id=$1`, [userId]);')).toEqual([]);
    expect(rules(source, 'db.prepare("INSERT INTO notes(body) VALUES(?)").bind("${customer360ProjectionSources.customer_portal_actions}");')).toEqual([]);
    expect(rules(source, 'const example=`SELECT * FROM (\\${customer360ProjectionSources.customer_portal_actions})`; const message="${customer360ProjectionSources.customer_portal_actions}"; describe(message);')).toEqual([]);
    expect(rules(source, 'owner.prepare("Review ${customer360ProjectionSources.customer_portal_actions}"); owner.query({message:"${customer360ProjectionSources.customer_portal_actions}"});')).toEqual([]);
  });
  test("requires a SET clause for UPDATE and checks interpolated query templates", () => {
    expect(rules(application, 'const steps=["Update the owning task with progress and blockers."];')).not.toContain("core-sql");
    expect(rules(domain, 'const query="update users set name=?";')).toContain("core-sql");
    expect(rules(domain, 'const query=\'UPDATE "the" SET name=?\';')).toContain("core-sql");
    expect(rules(domain, 'const query=`UPDATE ${table} SET name=${name}`;')).toContain("core-sql");
    expect(rules(domain, 'const query=`SELECT ${fields} FROM users`;')).toContain("core-sql");
  });
  test("recognizes structured error instructions while retaining SQL literal enforcement", () => {
    expect(rules(application, 'fail(404, "DELIVERY_NOT_FOUND", "Select a current delivery from this checkout.");')).toEqual([]);
    expect(rules(application, 'throw new BusinessRuleError(409,"MANAGEMENT_CHANGED","Update active group rules before archiving this field or changing its choices.");')).toEqual([]);
    expect(rules(application, 'const query="Update active group rules before archiving this field or changing its choices.";')).not.toContain("core-sql");
    expect(rules(application, 'throw new BusinessRuleError(409,"MANAGEMENT_CHANGED","UPDATE groups SET archived=true");')).toContain("core-sql");
    expect(rules(application, 'throw new BusinessRuleError(422,"PROMISE_LINE_INVALID","Select a line from this purchase order.");')).toEqual([]);
    expect(rules(application, 'ports.error({status:409,code:"DELIVERY_REQUIRED",message:"Select the delivery from this checkout."});')).toEqual([]);
    expect(rules(application, 'throw ports.error(422,"BANK_ACCOUNT_INVALID","Select an active cash or bank account from the primary book. Secondary books use the same business statement.");')).toEqual([]);
    expect(rules(application, 'throw ports.error(422,"FX_ACCOUNT_INVALID","Select functional-currency other-income and other-expense accounts from this ledger.");')).toEqual([]);
    expect(rules(application, 'throw ports.error(409,"INTERCOMPANY_POSTED_DOCUMENT_REQUIRED","Select original posted document accounts from two active Group Companies in the same transaction currency.");')).toEqual([]);
    expect(rules(application, 'const query="Select original posted document accounts from two active Group Companies in the same transaction currency.";')).toContain("core-sql");
    expect(rules(application, 'throw ports.error(409,"INTERCOMPANY_POSTED_DOCUMENT_REQUIRED","SELECT account_id FROM finance_journal_lines");')).toContain("core-sql");
    expect(rules(application, 'const query="Select functional-currency other-income and other-expense accounts from this ledger.";')).toContain("core-sql");
    expect(rules(application, 'throw ports.error(422,"FX_ACCOUNT_INVALID","SELECT functional-currency FROM finance_accounts;");')).toContain("core-sql");
    expect(rules(application, 'const query="Select a current delivery from this checkout.";')).toContain("core-sql");
    expect(rules(application, 'fail(404,"DELIVERY_NOT_FOUND","SELECT a FROM deliveries");')).toContain("core-sql");
    expect(rules(application, 'throw new BusinessRuleError(422,"PROMISE_LINE_INVALID","SELECT a FROM deliveries");')).toContain("core-sql");
    expect(rules(application, 'new Query(422,"PROMISE_LINE_INVALID","Select a line from this purchase order.");')).toContain("core-sql");
    expect(rules(application, 'ports.error({status:409,code:"DELIVERY_REQUIRED",message:"SELECT * FROM deliveries"});')).toContain("core-sql");
    expect(rules(application, 'fail(404,"delivery","Select a current delivery from this checkout.");')).toContain("core-sql");
  });
  test("rejects another owner's private adapter and public runtime facade values", () => {
    expect(rules(application, 'import {save} from "@erpx/server-finance/adapters/repository";')).toContain("module-private-import");
    expect(rules(domain, 'import {publish} from "@erpx/server-brand/application/publish";')).toContain("core-dependency-direction");
    expect(rules(application, 'import {post} from "@/server/modules/finance";')).toContain("core-runtime-facade");
  });
  test("permits narrow shared ports and type-only owner contracts", () => {
    expect(rules(application, 'import type {Clock} from "@/server/modules/shared"; import type {PersistedInvoiceLine} from "@erpx/server-finance/public/finance-invoice-lines";')).toEqual([]);
    expect(rules(application, 'import {type PersistedInvoiceLine} from "@erpx/server-finance/public/finance-invoice-lines";')).toEqual([]);
    expect(rules("src/app/example.ts",'import {missing} from "@erpx/server-finance/public/missing";')).toContain("public-entry-missing");
  });
  test("rejects machine-specific paths in every dependency form including inline types",()=>{
    const adapter="packages/server-brand/src/adapters/repository.ts";
    for(const text of [
      'import type {Facts} from "/Users/operator/work/erpx/src/server/contracts";',
      'export {save} from "/opt/work/erpx/src/server/service";',
      'const service=await import("/Users/operator/work/erpx/src/server/service");',
      'const service=require("C:/repo/erpx/service");',
      'type Facts=import("file:///Users/operator/erpx/src/server/contracts").Facts;',
      'type Facts=import("/Users/operator/erpx/src/server/contracts").Facts;',
    ])expect(rules(adapter,text)).toContain("nonportable-source-import");
    expect(rules(application,'type Clock=import("@/server/modules/shared").Clock;')).toEqual([]);
    expect(rules(application,'type Facts=import("@erpx/server-finance/adapters/repository").Facts;')).toContain("module-private-import");
    const issues=inspectSource(root,adapter,'type Facts=import("/Users/operator/erpx/src/server/contracts").Facts;');
    expect(evaluateBaseline(issues,issues.map(issue=>issue.id)).violations).toHaveLength(1);
  });
  test("rejects infrastructure concealed in a transitive portable library",()=>{
    const temporary=fs.mkdtempSync(path.join(os.tmpdir(),"erpx-core-boundary-"));
    try{
      fs.mkdirSync(path.join(temporary,"packages/server-brand/src/domain"),{recursive:true});fs.mkdirSync(path.join(temporary,"src/lib"),{recursive:true});
      fs.writeFileSync(path.join(temporary,"packages/server-brand/src/domain/policy.ts"),'import {clock} from "@/lib/clock"; export const calculate=()=>clock();');
      fs.writeFileSync(path.join(temporary,"src/lib/clock.ts"),'import {readFileSync} from "fs"; export const clock=()=>Date.now();');
      const found=inspectArchitecture(temporary).filter(issue=>issue.rule==="core-transitive-dependency");
      expect(found.map(issue=>issue.target)).toEqual(expect.arrayContaining([expect.stringContaining("core-global-effect: Date.now"),expect.stringContaining("core-infrastructure: fs")]));
      expect(found.every(issue=>!issue.grandfatherable)).toBe(true);
      fs.writeFileSync(path.join(temporary,"packages/server-brand/src/domain/policy.ts"),'import type {clock} from "@/lib/clock";');expect(inspectArchitecture(temporary)).toEqual([]);
    }finally{fs.rmSync(temporary,{recursive:true,force:true});}
  });
  test("allows public rule contracts only when their complete dependency closure stays pure",()=>{
    const temporary=fs.mkdtempSync(path.join(os.tmpdir(),"erpx-pure-rule-"));
    try{
      for(const folder of ["brand/application","sales/public/rules","sales/domain","sales/adapters"]){const [owner,...parts]=folder.split("/");fs.mkdirSync(path.join(temporary,"packages",`server-${owner}`,"src",...parts),{recursive:true});}
      fs.mkdirSync(path.join(temporary,"src/lib"),{recursive:true});
      fs.writeFileSync(path.join(temporary,"packages/server-brand/src/application/policy.ts"),'import {calculate} from "@erpx/server-sales/public/rules/policy"; export const run=()=>calculate(4);');
      const facade=path.join(temporary,"packages/server-sales/src/public/rules/policy.ts");
      fs.writeFileSync(facade,'export {calculate} from "../../domain/policy";');
      fs.writeFileSync(path.join(temporary,"packages/server-sales/src/domain/policy.ts"),'export const calculate=(value:number)=>value*2;');
      expect(inspectArchitecture(temporary)).toEqual([]);
      fs.writeFileSync(facade,'export {calculate} from "../../adapters/policy";');
      fs.writeFileSync(path.join(temporary,"packages/server-sales/src/adapters/policy.ts"),'export const calculate=()=>4;');
      expect(inspectArchitecture(temporary).some(issue=>issue.rule==="core-transitive-dependency"&&issue.target.includes("core-dependency-direction"))).toBe(true);
      fs.writeFileSync(facade,'export {calculate} from "@/lib/hidden-rule";');
      fs.writeFileSync(path.join(temporary,"src/lib/hidden-rule.ts"),'export const calculate=()=>Date.now();');
      expect(inspectArchitecture(temporary).some(issue=>issue.rule==="core-transitive-dependency"&&issue.target.includes("core-global-effect: Date.now"))).toBe(true);
    }finally{fs.rmSync(temporary,{recursive:true,force:true});}
  });
  test("a core violation is never grandfathered and exact legacy imports cannot grow", () => {
    const core = inspectSource(root, domain, 'import {holdingResources} from "@/server/runtime";');
    expect(evaluateBaseline(core, core.map((issue) => issue.id)).violations.length).toBeGreaterThan(0);
    const old = inspectSource(root, "src/server/benefits.ts", 'import {learningWorkspace} from "@/lib/people_payroll/people-roster-domain";');
    expect(evaluateBaseline(old, old.map((issue) => issue.id)).violations).toEqual([]);
    const extra = inspectSource(root, "src/server/benefits.ts", 'import {learningWorkspace,createLearningCourse} from "@/lib/people_payroll/people-roster-domain";');
    expect(evaluateBaseline(extra, old.map((issue) => issue.id)).violations).toHaveLength(1);
    expect(evaluateBaseline([], old.map((issue) => issue.id)).stale).toEqual(old.map((issue) => issue.id));
  });
  test("rejects wildcard module exports and local dynamic private imports", () => {
    expect(rules("packages/server-brand/src/index.ts", 'export * from "../../brand-identity";')).toContain("public-wildcard-export");
    expect(rules("packages/server-brand/src/adapters/repository.ts", 'const other=await import("@erpx/server-finance/adapters/repository");')).toContain("module-private-import");
  });
});


test("projection and reporting infrastructure remain required for semantic completion",()=>{
 const owner=(id:string,kind:"business"|"projection"|"infrastructure",migration:"legacy"|"clean")=>({id,kind,migration,modules:[],source:[]});
 for(const pending of [owner("work_projection","projection","legacy"),owner("runtime","infrastructure","legacy")]){
  const status=architectureStatus("/unavailable-erpx-status-fixture",{version:1,modules:[],owners:[owner("brand","business","clean"),pending]});
  expect(status.owners.map(item=>item.owner)).toContain(pending.id);
  expect(status.complete).toBe(false);
 }
 const complete=architectureStatus("/unavailable-erpx-status-fixture",{version:1,modules:[],owners:[owner("brand","business","clean"),owner("work_projection","projection","clean"),owner("runtime","infrastructure","clean")]});
 expect(complete.complete).toBe(true);
});

function capabilityStatusInventory(migration:"clean"|"partial"|"legacy"|undefined):typeof serviceOwnership {
 const capability={id:"brand-policy",migration:migration??"clean",source:[],regression:[]};
 // A JSON register can omit a declared capability's state despite the TS type.
 if(migration===undefined)Reflect.deleteProperty(capability,"migration");
 return {version:1,modules:[],owners:[
  {id:"brand",kind:"business",migration:"clean",modules:[],source:[],capabilities:[capability]},
  {id:"shared",kind:"kernel",migration:"clean",modules:[],source:[]},
 ]};
}

test("declared incomplete capabilities prevent clean owner completion",()=>{
 for(const migration of ["partial","legacy",undefined] as const){
  const status=architectureStatus("/unavailable-erpx-status-fixture",capabilityStatusInventory(migration));
  expect(status.owners[0]).toMatchObject({migration:"clean",legacyImplementations:[],cleanCapabilities:[],incompleteCapabilities:[{id:"brand-policy",migration:migration??"unset"}]});
  expect(status.complete).toBe(false);
 }
 const complete=architectureStatus("/unavailable-erpx-status-fixture",capabilityStatusInventory("clean"));
 expect(complete.complete).toBe(true);
 expect(complete.owners[0]).toMatchObject({cleanCapabilities:["brand-policy"],incompleteCapabilities:[]});
 expect(complete.owners[1]).toMatchObject({owner:"shared",cleanCapabilities:[],incompleteCapabilities:[]});
});

test("require-clean CLI rejects incomplete capability states and accepts clean controls",()=>{
 const directory=fs.mkdtempSync(path.join(os.tmpdir(),"erpx-capability-status-cli-"));
 try{
  fs.mkdirSync(path.join(directory,"scripts/architecture"),{recursive:true});
  fs.mkdirSync(path.join(directory,"packages/contracts/src"),{recursive:true});
  fs.mkdirSync(path.join(directory,"node_modules/@erpx"),{recursive:true});
  fs.symlinkSync("../../packages/contracts",path.join(directory,"node_modules/@erpx/contracts"),"dir");
  fs.copyFileSync(path.join(root,"packages/contracts/package.json"),path.join(directory,"packages/contracts/package.json"));
  // Execute copies of the actual command and registry reader, without a mirror implementation.
  for(const source of ["scripts/architecture/status.ts","packages/contracts/src/ownership.ts"])fs.copyFileSync(path.join(root,source),path.join(directory,source));
  for(const migration of ["partial","legacy",undefined,"clean"] as const){
   fs.writeFileSync(path.join(directory,"packages/contracts/src/ownership.json"),JSON.stringify(capabilityStatusInventory(migration)));
   const result=spawnSync(pinnedBunBinary(root),["--env-file=/dev/null",path.join(directory,"scripts/architecture/status.ts"),"--require-clean"],{cwd:directory,env:process.env,timeout:10_000});
   expect(result.error).toBeUndefined();
   expect(result.status,result.stderr.toString()).toBe(migration==="clean"?0:1);
   const status=JSON.parse(result.stdout.toString()) as ReturnType<typeof architectureStatus>;
   expect(status.complete).toBe(migration==="clean");
   expect(status.owners[0].incompleteCapabilities).toEqual(migration==="clean"?[]:[{id:"brand-policy",migration:migration??"unset"}]);
  }
 }finally{fs.rmSync(directory,{recursive:true,force:true});}
});

test("full architecture acceptance rejects incomplete semantic owners after boundaries pass",async()=>{
 const commands:string[][]=[];
 await expect(runArchitectureAcceptance(async command=>{commands.push(command);return command[0].endsWith("status.ts")?1:0;})).rejects.toThrow("ARCHITECTURE_ACCEPTANCE_FAILED:scripts/architecture/status.ts");
 expect(commands).toEqual([["scripts/architecture/check.ts"],["scripts/architecture/status.ts","--require-clean"]]);
 await expect(runArchitectureAcceptance(async()=>1)).rejects.toThrow("ARCHITECTURE_ACCEPTANCE_FAILED:scripts/architecture/check.ts");
 await expect(runArchitectureAcceptance(async()=>0)).resolves.toBeUndefined();
});
