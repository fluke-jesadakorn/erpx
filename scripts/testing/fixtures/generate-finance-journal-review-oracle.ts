/** Development-only B0 oracle. Production imports none of this harness. */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const sourceCommit = "a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const paths = [
  "packages/server-finance/src/application/journal-review.ts",
  "packages/server-finance/src/domain/finance-policy.ts",
  "packages/server-finance/src/adapters/journal-reversal-money.ts",
  "packages/server-finance/src/adapters/finance.ts",
  "packages/server-finance/src/adapters/document-purchases.ts",
  "packages/server-finance/src/adapters/finance-transactions.ts",
  "packages/feature-identity_access/src/lib/permissions.ts",
  "packages/feature-identity_access/src/lib/iam.ts",
  "packages/feature-identity_access/src/lib/business-roles.ts",
  "packages/catalogue/src/module-registry.ts",
  "packages/catalogue/src/module-registry.json",
  "packages/feature-finance/src/lib/finance-money.ts",
];
const sources = [];
const baselineSources = new Map<string, string>();
for (const path of paths) {
  const bytes = execFileSync("git", ["show", `${sourceCommit}:${path}`]);
  baselineSources.set(resolve(path), bytes.toString("utf8"));
  sources.push({ path, sha256: createHash("sha256").update(bytes).digest("hex") });
}
const node = process.env.ERPX_NODE_BIN ?? "node";
if (execFileSync(node, ["--version"], { encoding: "utf8" }).trim() !== "v26.11.1") throw new Error("PINNED_FINANCE_REVIEW_NODE_REQUIRED");
const directory = resolve(".scratch/dev/finance-journal-review-oracle");
await mkdir(directory, { recursive: true, mode: 0o700 });
const reversalSource = baselineSources.get(resolve(paths[2]!))!;
function exactSlice(start: string, end: string) {
  const a = reversalSource.indexOf(start), b = reversalSource.indexOf(end, a);
  if (a < 0 || b <= a) throw new Error("FINANCE_REVIEW_ORACLE_HELPER_BOUNDARY_CHANGED");
  return reversalSource.slice(a, b);
}
// Execute the actual private functions, without importing production adapters,
// opening a database, or copying their financial decisions into this harness.
const privateDeclarations = exactSlice("type Row=", "async function authority");
const privateFunctions = exactSlice("function ancestry(unit:Unit)", "function validateNew(unit:Unit");
const entry = resolve(directory, "entry.ts");
await writeFile(entry, `
import {createJournalReview} from ${JSON.stringify(resolve(paths[0]!))};
import {hasPermission} from ${JSON.stringify(resolve(paths[6]!))};
import {readFileSync} from "node:fs";
class ApiError extends Error { constructor(status,code,message){super(message);this.status=status;this.code=code;} }
${privateDeclarations}
const requireFinanceMoneyScale=async()=>2;
const snapshotMoneyReversal=async(_db,_actor,input)=>({statement:{mode:"reverse_positive",input}});
const snapshotMoneyCarry=async(_db,_actor,input)=>({statement:{mode:"carry_positive",input}});
const snapshotMoney=async(_db,_actor,input)=>({statement:{mode:"capture_new",input},usdUnits:"919",rateId:"synthetic-new-rate",usedLatestFallback:false});
${privateFunctions}
function failure(error){return {status:error.status??null,code:error.code??null,message:error.message};}
const input=JSON.parse(readFileSync(0,"utf8"));
const reviews=[];
for(const row of input.reviews){
 const trace=[];const actor={userId:"reviewer",companyId:"company",assignmentId:"assignment",role:"finance_approver",iam:{permissions:row.permissions,personUserIds:row.personUserIds}};
 const mark=(name,value)=>{trace.push(name);return value;};
 const app=createJournalReview({
  error:(...args)=>new ApiError(...args),authorize:(actor,permission)=>{trace.push("authorize");if(!hasPermission(actor,permission))throw new ApiError(403,"PERMISSION_DENIED","Synthetic current authority denied.");return actor;},
  now:()=>1800000000000,ready:async()=>{trace.push("ready");},replay:async()=>mark("replay",row.replay??null),
  journal:async()=>mark("journal",row.journal),postedJournal:async()=>mark("postedJournal",row.journal?.status==="posted"?row.journal:null),debit:async()=>mark("debit",{amount:row.debit}),
  openingAllocated:async()=>mark("openingAllocated",row.facts.openingAllocated),sourceOpening:async()=>mark("sourceOpening",row.facts.sourceOpening),reversalRoot:async()=>mark("reversalRoot",row.facts.rootSourceType?{source_type:row.facts.rootSourceType}:null),
  commitPost:async facts=>mark("commitPost",{plan:facts.plan,result:{id:facts.journalId,status:"posted",version:facts.input.expectedVersion+1}}),
  commitReverse:async facts=>mark("commitReverse",{opening:facts.opening,now:facts.now,result:{reversalJournalId:"reversal-synthetic-command",reversedJournalId:facts.journalId}}),
 });
 try{const value=await(row.action==="post"?app.postJournal(actor,"journal","synthetic-command",row.input):app.reverseJournal(actor,"journal","synthetic-command",row.input));reviews.push({...row,expected:{value},trace});}
 catch(error){reviews.push({...row,expected:{error:failure(error)},trace});}
}
const money=[];
for(const row of input.money){
 const actor={companyId:"company",userId:"reviewer"};
 const database={prepare:query=>({bind:(...args)=>({mode:"invert_negative",args,query})})};
 const chain=row.chain.map(r=>({...r,source_type:r.sourceType,source_id:r.sourceId,source:JSON.stringify({entry_date:r.entryDate})}));
 const snapshots=row.money.map(m=>Object.fromEntries(Object.entries(m).map(([key,value])=>[key.replace(/[A-Z]/g,c=>"_"+c.toLowerCase()),value])));
 const unit={actor,actorImage:JSON.stringify(actor),journalId:chain[0]?.id??"journal",key:"synthetic-command",before:{chain,money:snapshots}};
 units.set(database,unit);
 try{
  const statement=await prepareJournalReversalMoney(database,actor,{journalId:unit.journalId,reversalId:"reversal-synthetic-command",key:unit.key,periodId:"period",entryDate:"2026-12-31",memo:"Synthetic reversal",at:1800000000000});
  const {rows,total,currency}=ancestry(unit),captured=sourceMoney(unit,rows,total,currency),values=unit.plan.money;
  const value={mode:statement.mode,sourceId:captured?.source.source_id??null,factor:captured?Number(BigInt(values.amount_units)/BigInt(captured.source.amount_units)):null,currency:values.currency,currencyScale:values.currency_scale,amountUnits:values.amount_units,usdUnits:captured?values.usd_units:null,fxRateId:captured?values.fx_rate_id:null,usedLatestFallback:captured?values.used_latest_fallback:null};
  money.push({...row,expected:{value}});
 }catch(error){money.push({...row,expected:{error:failure(error)}});}
 finally{units.delete(database);}
}
process.stdout.write(JSON.stringify({node:process.version,reviews,money}));
`, { mode: 0o600 });
const built = await Bun.build({
  entrypoints: [entry], outdir: directory, target: "node", format: "esm", naming: "oracle.mjs",
  plugins: [{ name: "exact-b0-finance-review-sources", setup(build) {
    build.onLoad({ filter: /\/packages\/.*\.(ts|json)$/ }, args => {
      const contents = baselineSources.get(args.path);
      if (contents === undefined) throw new Error(`UNRETAINED_FINANCE_REVIEW_ORACLE_IMPORT:${args.path}`);
      return { contents, loader: args.path.endsWith(".json") ? "json" : "ts" };
    });
  } }],
});
if (!built.success) throw new Error(`FINANCE_REVIEW_ORACLE_BUILD_FAILED:${built.logs.join("\n")}`);
const journal = { id: "journal", company_id: "company", chart_id: "chart", period_id: "period", journal_number: "J-1", entry_date: "2026-01-01", currency: "THB", status: "draft", version: 1, created_by: "preparer", source_type: "manual", source_id: null };
const permissions = ["finance.read", "finance.write", "finance.approve"];
const base = { action: "post", permissions, personUserIds: ["reviewer"], journal, input: { expectedVersion: 1 }, debit: 12345, facts: { openingAllocated: false, sourceOpening: false, rootSourceType: "manual" } };
const reviews: Record<string, unknown>[] = [
  { id: "post-manual", ...base }, { id: "post-opening", ...base, journal: { ...journal, source_type: "opening_import" } },
  { id: "post-no-journal", ...base, journal: null }, { id: "post-own-user", ...base, journal: { ...journal, created_by: "reviewer" } },
  { id: "post-same-person-alias", ...base, personUserIds: ["reviewer", "preparer"] },
  { id: "post-no-approve", ...base, permissions: ["finance.read", "finance.write"] },
  { id: "post-no-read-but-current-write-approve", ...base, permissions: ["finance.write", "finance.approve"] },
  { id: "post-no-write", ...base, permissions: ["finance.read", "finance.approve"] },
  { id: "post-replay-before-independence", ...base, personUserIds: ["preparer"], replay: { id: "journal", status: "posted", version: 2 } },
];
for (const sourceType of ["manual", "opening_import", "reversal", "invoice", "payment", "book_projection", "book_adjustment"]) {
  for (const entryDate of ["2025-12-31", "2026-01-01", "2026-02-01"]) reviews.push({ ...base, id: `reverse-${sourceType}-${entryDate}`, action: "reverse", journal: { ...journal, source_type: sourceType, source_id: sourceType === "reversal" ? "parent" : null, status: "posted" }, input: { periodId: "period", entryDate } });
}
const reverse = { ...base, action: "reverse", journal: { ...journal, status: "posted" }, input: { periodId: "period", entryDate: "2026-01-01" } };
reviews.push(
  { ...reverse, id: "reverse-same-person-is-allowed", personUserIds: ["preparer"] },
  { ...reverse, id: "reverse-no-read", permissions: ["finance.write", "finance.approve"] },
  { ...reverse, id: "reverse-no-approve", permissions: ["finance.read", "finance.write"] },
  { ...reverse, id: "reverse-missing", journal: null },
  { ...reverse, id: "reverse-not-posted", journal },
  { ...reverse, id: "reverse-opening-allocated-before-date", journal: { ...journal, status: "posted", source_type: "opening_import" }, input: { periodId: "period", entryDate: "2025-12-31" }, facts: { ...base.facts, openingAllocated: true } },
  { ...reverse, id: "reverse-opening-reversal", journal: { ...journal, status: "posted", source_type: "reversal", source_id: "parent" }, facts: { ...base.facts, sourceOpening: true } },
  ...[null, "opening_import", "invoice"].map(rootSourceType => ({ ...reverse, id: `reverse-invalid-root-${rootSourceType}`, journal: { ...journal, status: "posted", source_type: "reversal", source_id: "parent" }, facts: { ...base.facts, rootSourceType } })),
  { ...reverse, id: "reverse-replay-before-missing", journal: null, replay: { reversalJournalId: "reversal-synthetic-command", reversedJournalId: "journal" } },
  { ...reverse, id: "reverse-replay-still-requires-approve", permissions: ["finance.read", "finance.write"], replay: { reversalJournalId: "reversal-synthetic-command", reversedJournalId: "journal" } },
);
type Chain = { id: string; companyId: string; depth: number; cycle: boolean; sourceType: string; sourceId: string | null; currency: string; total: string; entryDate: string };
type Money = { companyId: string; sourceId: string; amountKind: string; transactionDate: string; currency: string; currencyScale: number; amountUnits: string; usdUnits: string; fxRateId: string | null; usedLatestFallback: boolean; originalSourceType: string | null; originalSourceId: string | null; originalAmountKind: string | null };
function chain(length: number, currency = "THB", total = "10000", root = "manual"): Chain[] {
  return Array.from({ length }, (_, i) => ({ id: i === 0 ? "journal" : `ancestor-${i}`, companyId: "company", depth: i, cycle: false, sourceType: i < length - 1 ? "reversal" : root, sourceId: i < length - 1 ? `ancestor-${i + 1}` : null, currency, total, entryDate: `2026-01-${String(length - i).padStart(2, "0")}` }));
}
function snapshot(rows: Chain[], index: number): Money {
  const row = rows[index]!, sign = (rows.length - 1 - index) % 2 ? -1n : 1n;
  const amountUnits = String(BigInt(row.total) * sign);
  return { companyId: "company", sourceId: row.id, amountKind: "debit_total", transactionDate: row.entryDate, currency: row.currency, currencyScale: 2, amountUnits, usdUnits: row.currency === "USD" ? amountUnits : String(3437n * sign), fxRateId: row.currency === "USD" ? null : "historical-rate", usedLatestFallback: row.currency !== "USD", originalSourceType: null, originalSourceId: null, originalAmountKind: null };
}
const money: { id: string; chain: Chain[]; money: Money[] }[] = [];
for (const currency of ["USD", "THB"]) for (let length = 1; length <= 8; length++) {
  const rows = chain(length, currency);
  money.push({ id: `${currency}-${length}-uncaptured`, chain: rows, money: [] });
  for (let index = 0; index < length; index++) money.push({ id: `${currency}-${length}-captured-${index}`, chain: rows, money: [snapshot(rows, index)] });
  money.push({ id: `${currency}-${length}-all-captured-prefers-current`, chain: rows, money: rows.map((_, i) => snapshot(rows, i)) });
}
const pair = chain(2), original = snapshot(pair, 1), negative = { ...snapshot(pair, 0), originalSourceType: "finance_journal", originalSourceId: original.sourceId, originalAmountKind: "debit_total" };
money.push({ id: "negative-with-true-positive-origin", chain: pair, money: [negative, original] });
for (const [field, value] of Object.entries({ currency: "EUR", currencyScale: 0, transactionDate: "2020-01-01", amountUnits: "10000", usdUnits: "3437", fxRateId: null, originalSourceType: "invoice", originalSourceId: "unrelated", originalAmountKind: "credit_total" })) money.push({ id: `invalid-current-${field}`, chain: pair, money: [{ ...negative, [field]: value }, original] as Money[] });
for (const [field, value] of Object.entries({ amountUnits: "10001", usdUnits: "3438", fxRateId: "other-rate", currency: "EUR", currencyScale: 0, usedLatestFallback: false })) money.push({ id: `invalid-positive-origin-${field}`, chain: pair, money: [negative, { ...original, [field]: value }] as Money[] });
const usd = chain(1, "USD");
money.push({ id: "invalid-USD-rate", chain: usd, money: [{ ...snapshot(usd, 0), fxRateId: "forged" }] }, { id: "invalid-USD-units", chain: usd, money: [{ ...snapshot(usd, 0), usdUnits: "10001" }] });
for (const [field, value] of Object.entries({ cycle: true, depth: 7, sourceId: "other", currency: "EUR", total: "10001" })) money.push({ id: `invalid-chain-${field}`, chain: [{ ...pair[0]!, [field]: value }, pair[1]!] as Chain[], money: [] });
money.push({ id: "missing-chain", chain: [], money: [] }, { id: "unsupported-root", chain: chain(1, "THB", "100", "invoice"), money: [] }, { id: "zero-total", chain: chain(1, "THB", "0"), money: [] }, { id: "negative-total", chain: chain(1, "THB", "-1"), money: [] });
for (const total of ["9007199254740993", "999999999999999999999999999999999999"]) { const rows = chain(4, "USD", total); money.push({ id: `exact-bigint-${total}`, chain: rows, money: [snapshot(rows, 3)] }); }
const output = JSON.parse(execFileSync(node, [resolve(directory, "oracle.mjs")], { input: JSON.stringify({ reviews, money }), encoding: "utf8", maxBuffer: 16 * 1024 * 1024, env: { PATH: process.env.PATH ?? "", NODE_ENV: "test", TZ: "UTC", LANG: "en_US.UTF-8" } }));
const document = `${JSON.stringify({ version: 1, sourceCommit, sources, node: output.node, scope: "actual B0 review application and private reversal Money functions; synthetic ports only, no database/FX/provider acceptance", reviews: output.reviews, money: output.money }, null, 2)}\n`;
const destination = "fixtures/finance/rust-journal-review/conformance.v1.json";
if (process.argv.includes("--check")) { if (await readFile(destination, "utf8") !== document) throw new Error("FINANCE_REVIEW_ORACLE_DRIFT"); }
else { await mkdir(resolve(destination, ".."), { recursive: true }); await writeFile(destination, document); }
console.log(`Finance journal oracle: ${output.reviews.length} review and ${output.money.length} exact Money-lineage cases${process.argv.includes("--check") ? " (unchanged)" : ""}.`);
