import {randomUUID} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';

type Actors={company:string;reviewer:WorkContext;manager:WorkContext};
const key=()=>randomUUID();

/**
 * Prepare a historical Company through the actual Supply and Finance owners.
 * The caller must bind the owner runtime to this exact verified leased database.
 * No mapping or Finance business source is inserted directly by the fixture.
 */
export async function seedHistoricalSupplyMapping(db:PostgresDatabase,z:Actors){
 const lease=await db.prepare("SELECT current_database() name,pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=current_database()").first<{name:string;owner:string;lease:string}>();
 if(!lease||!/^erpx_dev_rust_[0-9a-f]{32}$/.test(lease.name)||lease.owner!=='erpx_test_admin'||lease.lease!==`erpx-rust-test:${lease.name.slice('erpx_dev_rust_'.length)}`)throw Error('HISTORICAL_SUPPLY_LEASE_REQUIRED');
 if(z.reviewer.companyId!==z.company||z.manager.companyId!==z.company||z.reviewer.userId===z.manager.userId)throw Error('HISTORICAL_SUPPLY_ACTORS_REQUIRED');
 const {packageGrantStore,packageGrantRevision,readCompanyPackageGrants,readCompanyPackageHistoryRights}=await import('@erpx/server-commercial/public/package-grants');
 const {withOwnerPackageObligations,ownerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');
 const finance=await import('@erpx/server-finance/public/finance'),money=await import('@erpx/server-finance/public/company-money'),supply=await import('@erpx/server-commerce/public/posting-setup');
 const grant={companyId:z.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Original Supply mapping before Service adoption',evidence:'test:historical-Service-parts-map'};
 const provider=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-existing-map-provider'}),obligations:(transaction,companyId,packageId)=>ownerPackageObligations(db,transaction,companyId,packageId)});
 await provider.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const chart=await finance.createChart(z.reviewer,{name:'Actual retained original Supply chart',baseCurrency:'THB',accountingBasis:'TFRS'},key()),accounts:Record<string,string>={};
 for(const [field,code,category,controlKind] of [['receivableAccountId','1100','asset','receivable'],['payableAccountId','2100','liability','payable'],['revenueAccountId','4100','revenue','none'],['inventoryAccountId','1300','asset','none'],['clearingAccountId','2150','liability','none'],['cogsAccountId','5100','expense','none'],['inputTaxAccountId','1150','asset','none'],['outputTaxAccountId','2151','liability','none'],['cashAccountId','1001','asset','bank'],['varianceAccountId','5400','expense','none']] as const){const result=await finance.createAccount(z.reviewer,{chartId:chart.id,code,name:`Actual original retained ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key());accounts[field]=String(result.id);}
 await finance.createPeriod(z.reviewer,{name:'Actual original Supply observation year',startDate:'2020-01-01',endDate:'2035-12-31'},key());
 await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Independent Company quote before Service confirmation'},key());
 const preview=await supply.commercePostingSetup(z.reviewer);if(!preview.contextKey)throw Error('HISTORICAL_SUPPLY_CONTEXT_REQUIRED');
 const nine=Object.fromEntries(Object.entries(accounts).filter(([field])=>field!=='varianceAccountId'));
 const command=key(),result=await supply.initializeCommercePostingSetup(z.reviewer,{expectedVersion:0,contextKey:preview.contextKey,chartId:chart.id,currency:'THB',reason:'Original Company owner selects the shared nine-role mapping',...nine},command);
 if(result.id!==z.company||result.status!=='configured'||result.version!==1)throw Error('HISTORICAL_SUPPLY_MAPPING_RECEIPT_INVALID');
 const image=await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image');if(!image)throw Error('HISTORICAL_SUPPLY_MAPPING_MISSING');
 const settlement={companyId:z.company,packageId:'supply',packageVersion:1};
 for(const [state,expectedVersion] of [['draining',1],['inactive',2]] as const){
  const grants=await readCompanyPackageGrants(db,z.company),request={companyId:z.company,expectedRevision:packageGrantRevision(z.company,grants),idempotencyKey:key(),changes:[{expectedVersion,grant:{...grant,state,version:expectedVersion+1,reason:state==='draining'?'Stop new Supply work before owner settlement':'Actual Supply obligations are terminal; retain mapping history',evidence:'test:real-Service-existing-map-Supply-settlement'}}]};
  const decision=await withOwnerPackageObligations(db,settlement,async transaction=>{const obligations=await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()});if(obligations.total!==0)throw Error('HISTORICAL_SUPPLY_OBLIGATIONS_REMAIN');return provider.batch.previewIn(transaction,request);});
  const applied=await withOwnerPackageObligations(db,settlement,transaction=>provider.batch.applyIn(transaction,request,async current=>{if(JSON.stringify(current)!==JSON.stringify(decision))throw Error('HISTORICAL_SUPPLY_SETTLEMENT_CHANGED');}));
  if(JSON.stringify(applied)!==JSON.stringify(decision))throw Error('HISTORICAL_SUPPLY_SETTLEMENT_MISMATCH');
 }
 const grants=await readCompanyPackageGrants(db,z.company),history=await readCompanyPackageHistoryRights(db,z.company),after=await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image');
 if(after!==image||!history.some(row=>row.packageId==='supply')||grants.find(row=>row.packageId==='supply')?.state!=='inactive')throw Error('HISTORICAL_SUPPLY_RETENTION_INVALID');
 return {chartId:chart.id,accounts,mappingCommandId:command,mappingImage:image};
}
