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

describe('Assets read-tail r34 universal select head family',()=>{
 it('renders the one universal head byte-identically for both delegates',async()=>{
  const {assetSelectProjectionSql,assetProjectionFromSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  const {assetSourceImageSql}=await import('../../../packages/server-assets/src/adapters/financial-workflow-sources');
  // Pre-r34 literals, rebuilt verbatim; no projection, FROM or tail fragment changes.
  const head=(projection:string,tail="")=>`SELECT ${projection}${tail}`;
  const projectionFrom=(projection:string,from:string,tail="")=>`SELECT ${projection} FROM${from}${tail}`;
  const image=(projection:string,tail="")=>`SELECT ${projection}::text image${tail}`;
  for(const [projection,tail] of [
   ['1',''],
   ['id,version',' WHERE id=? AND company_id=?'],
   ['*',' FOR SHARE'],
  ] as const){
   expect(assetSelectProjectionSql(projection,tail)).toBe(head(projection,tail));
  }
  for(const [projection,from,tail] of [
   ['id,version',' operational_assets',''],
   ['asset_metric_owned.*',' maintenance_work_orders m',' WHERE m.company_id=?'],
   ['x',' y',' WHERE z'],
  ] as const){
   expect(assetProjectionFromSql(projection,from,tail)).toBe(projectionFrom(projection,from,tail));
   expect(assetProjectionFromSql(projection,from,tail)).toBe(assetSelectProjectionSql(projection,` FROM${from}${tail}`));
  }
  for(const [projection,tail] of [
   ["jsonb_build_object('asset',to_jsonb(r))",' FROM operational_assets r WHERE r.id=?'],
   ['1',''],
  ] as const){
   expect(assetSourceImageSql(projection,tail)).toBe(image(projection,tail));
   expect(assetSourceImageSql(projection,tail)).toBe(assetSelectProjectionSql(projection,`::text image${tail}`));
  }
 });

 it('keeps both real image-witness call sites byte-identical with unchanged binds',async()=>{
  const {assetSourceImageSql,assetFinanceLinkReadSql}=await import('../../../packages/server-assets/src/adapters/financial-workflow-sources');
  const {assetSourceProbeSql}=await import('../../../packages/server-assets/src/adapters/record-purchases');
  const agg=(alias:string,from:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})`;
  const probe=(from:string,where:string)=>`SELECT 1 FROM ${from} WHERE ${where}`;
  // Physical lineage witness: pre-r34 projection and tail, rebuilt verbatim.
  const physicalProjection=`jsonb_build_object('assets',${agg("a","physical a")},'events',${agg("e","operational_asset_events e JOIN physical a ON a.id=e.asset_id")},'links',${agg("l","operational_asset_finance_links l WHERE l.finance_asset_id=?")},'work',${agg("w","maintenance_work_orders w JOIN physical a ON a.id=w.asset_id")})`;
  const lineageTail=`,
  EXISTS(${probe("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id","l.finance_asset_id=? AND a.id IS NULL")})
  OR EXISTS(${probe("maintenance_work_orders w JOIN physical a ON a.id=w.asset_id","w.company_id<>a.company_id")}) invalid`;
  const lineage=`SELECT ${physicalProjection}::text image${lineageTail}`;
  expect(assetSourceImageSql(physicalProjection,lineageTail)).toBe(lineage);
  // The full prepared statement keeps its WITH prefix and all four binds byte-identical.
  const before='WITH physical AS MATERIALIZED(SELECT a.* FROM operational_assets a WHERE a.company_id=? AND a.finance_asset_id=?)';
  expect(assetFinanceLinkReadSql("a.*","a")).toBe(before.slice('WITH physical AS MATERIALIZED('.length,-1));
  const full=`${before}\n  ${lineage}`;
  expect(`${before}\n  ${assetSourceImageSql(physicalProjection,lineageTail)}`).toBe(full);
  expect((full.match(/\?/g)??[])).toHaveLength(4);
  // Purchase owner witness: pre-r34 projection and tail, rebuilt verbatim.
  const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";
  const purchaseProjection=`jsonb_build_object('asset',to_jsonb(r)-'version'-'updated_at','finance',to_jsonb(f),'links',${agg("link","operational_asset_finance_links link WHERE link.asset_id=r.id")})`;
  const purchaseTail=` FROM operational_assets r LEFT JOIN finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id WHERE r.id=? AND ${scope}`;
  expect(assetSourceImageSql(purchaseProjection,purchaseTail))
   .toBe(`SELECT ${purchaseProjection}::text image${purchaseTail}`);
 });

 it('owns one universal head literal with no local statement token in either delegate',()=>{
  const scope=src('adapters/asset-scope-tuple.ts');
  expect(scope.split('SELECT ${projection}${tail}').length-1).toBe(1);
  expect((scope.match(/SELECT/g)??[])).toHaveLength(2);
  expect(scope).toContain('export const assetSelectProjectionSql = (projection:string,tail="") =>');
  expect(scope).toContain('assetSelectProjectionSql(projection,` FROM${from}${tail}`)');
  expect(scope.split('SELECT ${projection} FROM${from}${tail}').length-1).toBe(0);
  const financial=src('adapters/financial-workflow-sources.ts');
  expect(financial).toContain('import {assetProjectionFromSql,assetSelectProjectionSql} from "./asset-scope-tuple"');
  expect(financial).toContain('export const assetSourceImageSql = (projection:string,tail="") =>');
  expect(financial).toContain('assetSelectProjectionSql(projection,`::text image${tail}`)');
  expect((financial.match(/SELECT/g)??[])).toHaveLength(0);
  expect(financial.split('SELECT ${projection}::text image${tail}').length-1).toBe(0);
 });

 it('lowers the exact owner TS SELECT tail from BEFORE=4 to KPI 3 and leaves excluded families untouched',()=>{
  expect(ownerSelectCount()).toBe(3);
  expect((src('adapters/record-purchases.ts').match(/SELECT/g)??[])).toHaveLength(1);
  expect((src('adapters/package-obligations.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('adapters/metric-purchases.ts').match(/SELECT/g)??[])).toHaveLength(0);
  const financial=src('adapters/financial-workflow-sources.ts');
  expect(financial).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(src('adapters/purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
  expect(src('adapters/package-obligations.ts')).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
 });
});
