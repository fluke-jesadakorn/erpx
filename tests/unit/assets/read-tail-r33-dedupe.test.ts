import {describe,it,expect,vi} from 'vitest';
import {readFileSync,readdirSync} from 'node:fs';
import {resolve} from 'node:path';

vi.mock('server-only',()=>({}));

const srcRoot=resolve(__dirname,'../../../packages/server-assets/src');
const src=(p:string)=>readFileSync(resolve(srcRoot,p),'utf8');
const ownerSelectCount=()=>{
 let selects=0;
 const walk=(dir:string)=>{
  for(const entry of readdirSync(dir,{withFileTypes:true})){
   const path=resolve(dir,entry.name);
   if(entry.isDirectory())walk(path);
   else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
  }
 };
 walk(srcRoot);
 return selects;
};

describe('Assets read-tail r33 projection-from head family',()=>{
 it('renders the physical lineage aggregate byte-identically from the one projection-from head',async()=>{
  const {assetPhysicalAggSql}=await import('../../../packages/server-assets/src/adapters/financial-workflow-sources');
  const {assetProjectionFromSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  // The pre-r33 literal, rebuilt verbatim; the a/e/l/w aliases and the purchase
  // owner's link alias must all render byte-identically after delegation.
  const before=(alias:string,from:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})`;
  const cases:[string,string][]=[
   ['a','physical a'],
   ['e','operational_asset_events e JOIN physical a ON a.id=e.asset_id'],
   ['l','operational_asset_finance_links l WHERE l.finance_asset_id=?'],
   ['w','maintenance_work_orders w JOIN physical a ON a.id=w.asset_id'],
   ['link','operational_asset_finance_links link WHERE link.asset_id=r.id'],
  ];
  for(const [alias,from] of cases){
   expect(assetPhysicalAggSql(alias,from)).toBe(before(alias,from));
   expect(`(${assetProjectionFromSql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,` ${from}`)})`).toBe(before(alias,from));
  }
  const text=src('adapters/financial-workflow-sources.ts');
  expect(text).toContain('assetProjectionFromSql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,` ${from}`)');
  expect(text.split('(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})').length-1).toBe(0);
  expect(text.split('assetProjectionFromSql(').length-1).toBe(2);
  // The Finance physical SHARE lock and the r20 image witness floor stay untouched.
  expect(text).toContain('export function assetsFinancialPhysicalSourceLocks');
  expect(text).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(text).toContain('export const assetSourceImageSql = (projection:string,tail="") =>');
 });

 it('renders the package settlement assets source byte-identically for the cross-owner maintenance consumer',async()=>{
  const {assetPackageSettlementSources}=await import('../../../packages/server-assets/src/adapters/package-obligations');
  const {assetProjectionFromSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  const {assetSourceProbeSql}=await import('../../../packages/server-assets/src/adapters/record-purchases');
  const {talentSettlementPlacements}=await import('@erpx/server-organization/public/talent-settlement-sources');
  const {projectionLookup}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
  const exists=(from:string,where:string)=>`EXISTS(${assetSourceProbeSql(from,where)})`;
  const sourceExists=(source:string,alias:string,where:string)=>exists(`(${source}) ${alias}`,where);
  const placements=talentSettlementPlacements;
  const branchPred="branch.id=asset.branch_id AND branch.company_id=asset.company_id AND branch.kind IN ('branch','headquarters')";
  const departmentPred="department.id=asset.department_id AND department.company_id=asset.company_id AND department.branch_id=asset.branch_id AND department.kind='department'";
  const projection=`asset.*,COALESCE(${sourceExists(placements,"branch",branchPred)} AND (asset.department_id IS NULL OR ${sourceExists(placements,"department",departmentPred)}),false) source_valid`;
  // The pre-r33 literal, rebuilt verbatim from the same owner-owned fragments.
  const before=`SELECT ${projection} FROM operational_assets asset`;
  expect(assetPackageSettlementSources.assets).toBe(before);
  expect(assetProjectionFromSql(projection," operational_assets asset")).toBe(before);
  // The cross-owner Maintenance consumer embeds this text in its fact-row CTE and
  // must observe identical bytes after the delegation.
  expect(projectionLookup('version',assetPackageSettlementSources.assets,'actual','actual.id=plan.asset_id AND actual.company_id=plan.company_id'))
   .toBe(`(SELECT version FROM (${before}) actual WHERE actual.id=plan.asset_id AND actual.company_id=plan.company_id)`);
  const text=src('adapters/package-obligations.ts');
  expect(text).toContain('import {assetProjectionFromSql} from "./asset-scope-tuple"');
  expect(text).toContain('assets:assetProjectionFromSql(`asset.*,COALESCE(');
  expect(text).toContain('source_valid`," operational_assets asset")');
  expect((text.match(/SELECT/g)??[])).toHaveLength(0);
  expect(text).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
  expect(text).toContain('links:assetAllRowsSql("operational_asset_finance_links")} as const;');
 });

 it('renders the metric source segment byte-identically with unchanged binds',async()=>{
  const {assetProjectionFromSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  const {assetClockNowMsSql}=await import('../../../packages/server-assets/src/adapters/record-purchases');
  const {assignmentReadSql}=await import('@erpx/server-identity_access/public/record-source-authority');
  const {purchasedCapabilityPredicate}=await import('@erpx/server-commercial/public/purchased-capability-sql');
  // The pre-r33 statement, rebuilt verbatim; only its SELECT segment moves.
  const before=(table:string,capability:string)=>
   `WITH asset_metric_clock AS MATERIALIZED ${assetClockNowMsSql} SELECT asset_metric_owned.* FROM ${table} asset_metric_owned CROSS JOIN asset_metric_clock WHERE ${assignmentReadSql("asset_metric_owned","records.read")} AND ${purchasedCapabilityPredicate("asset_metric_owned.company_id",capability,"history","asset_metric_clock.now_ms")}`;
  for(const [table,capability] of [
   ['operational_assets','assets.register'],
   ['maintenance_work_orders','assets.maintenance'],
   ['maintenance_work_orders','assets.downtime'],
  ] as const){
   const composed=`WITH asset_metric_clock AS MATERIALIZED ${assetClockNowMsSql} ${assetProjectionFromSql("asset_metric_owned.*",` ${table} asset_metric_owned CROSS JOIN asset_metric_clock`,` WHERE ${assignmentReadSql("asset_metric_owned","records.read")} AND ${purchasedCapabilityPredicate("asset_metric_owned.company_id",capability,"history","asset_metric_clock.now_ms")}`)}`;
   expect(composed).toBe(before(table,capability));
  }
  const text=src('adapters/metric-purchases.ts');
  expect(text).toContain('import {assetProjectionFromSql} from "./asset-scope-tuple"');
  expect(text).toContain('assetProjectionFromSql("asset_metric_owned.*",` ${definition.table} asset_metric_owned CROSS JOIN asset_metric_clock`');
  expect(text).toContain('assignmentReadSql("asset_metric_owned","records.read")');
  expect(text).toContain('purchasedCapabilityPredicate("asset_metric_owned.company_id",definition.capability,"history","asset_metric_clock.now_ms")');
  expect(text).toContain('args:[actor.assignmentId,actor.userId,actor.role]');
  expect((text.match(/SELECT/g)??[])).toHaveLength(0);
 });

 it('lowers the owner TS SELECT tail from BEFORE=7 to 4 and keeps the four floor tokens untouched',()=>{
  expect(ownerSelectCount()).toBe(4);
  // Floor 1 and 2: the canonical head and the scope tuple, both in asset-scope-tuple.ts.
  const scope=src('adapters/asset-scope-tuple.ts');
  expect((scope.match(/SELECT/g)??[])).toHaveLength(2);
  expect(scope.split('SELECT ${projection} FROM${from}${tail}').length-1).toBe(1);
  expect(scope.split('(SELECT ?::text company_id,?::text branch_id,?::text department_id)').length-1).toBe(1);
  // Floor 3: the clock scalar.
  expect((src('adapters/record-purchases.ts').match(/SELECT/g)??[])).toHaveLength(1);
  // Floor 4: the image witness.
  expect((src('adapters/financial-workflow-sources.ts').match(/SELECT/g)??[])).toHaveLength(1);
  // The three delegated files carry no local statement token.
  expect((src('adapters/package-obligations.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('adapters/metric-purchases.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect(src('adapters/financial-workflow-sources.ts').split('(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})').length-1).toBe(0);
  // Locks, native plans and earlier families are not re-opened.
  expect(src('adapters/purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
 });
});
