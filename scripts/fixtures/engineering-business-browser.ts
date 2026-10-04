/** Native business sources for one disposable normal-login browser run. */
import type {WorkContext} from '../../src/server/authorization';
import type {SqlDatabase} from '../../src/server/runtime/types';
import {PostgresDatabase} from '../../src/server/runtime/postgres';
import {moduleRegistry} from '../../src/data/module-registry';
import type {seedExpenseCompany} from '../../fixtures/expenses/company';
import {saveProject,projectSchema,saveTask,taskSchema} from '../../src/server/staff-tasks';
import {saveProjectMilestone,projectMilestoneSchema} from '../../src/server/project-milestones';
import {createProjectCostAllocation,decideProjectCostAllocation,projectCostAllocationSchema,projectCostAllocationDecisionSchema} from '../../src/server/project-actuals';
import {createAccount,createAccountSchema,createJournal,createJournalSchema,postJournal,importBankStatement} from '../../src/server/finance';
import {saveMaster,saveOrder,orderSchema,transitionOrder} from '../../src/server/commerce';
import {assignFlowPrimary,resolveFlow} from '../../src/server/flow-routing';
import {saveCount,transitionCount,countSchema} from '../../src/server/inventory-counts';
import {createAsset} from '../../src/server/asset-maintenance';
import {savePreventivePlan} from '../../src/server/maintenance-preventive';
import {createSpecification,reviewSpecification} from '../../src/server/production';
import {saveProductionResource,saveProductionResourceCalendar} from '../../src/server/production-resources';
import {createManufacturingAccountingProfile,reviewManufacturingAccountingProfile} from '../../src/server/production-accounting';
import {createChannel,createListing,createCart,addCartLine,changeEcommerce} from '../../src/server/ecommerce';
import {createSupportTicket} from '../../src/server/support';
import {createAgreement,changeAgreement,createServiceOrder} from '../../src/server/field-service';
import {posCommand} from '../../src/server/pos-core';
import {requestCustomerAccess} from '../../src/server/customer-access';

type Fixture=Awaited<ReturnType<typeof seedExpenseCompany>>;
type Input={run:string;db:SqlDatabase;f:Fixture};
const key=()=>crypto.randomUUID();
export async function seedEngineeringBusinessBrowser({run,db,f}:Input){
 if(!/^[a-f0-9]{12}$/.test(run)||!(db instanceof PostgresDatabase))throw new Error('EXACT_ENGINEERING_BROWSER_DATABASE_REQUIRED');
 const connection=new URL(String(db.pool.options.connectionString));
 if(connection.hostname!=='127.0.0.1'||connection.port!=='5432'||connection.username!=='erpx_test_admin'||connection.pathname!==`/erpx_browser_${run}_h001_db`)throw new Error('EXACT_ENGINEERING_BROWSER_DATABASE_REQUIRED');
 const modules=moduleRegistry.map(module=>module.id),today=new Date().toISOString().slice(0,10),now=Date.now();
 async function normalOwner(person:WorkContext){
  const assignment=await db.prepare("SELECT id FROM assignments WHERE company_id=? AND user_id=? AND role='holding_owner' AND iam_assignment_active(id,?) ORDER BY valid_from DESC,id DESC LIMIT 1").bind(f.company,person.userId,now).first<{id:string}>();
  if(!assignment)throw new Error('INDEPENDENT_NORMAL_BROWSER_OWNER_GRANT_REQUIRED');
  return {...person,role:'holding_owner',assignmentId:assignment.id,holdingSlug:'simulation',branchId:null,departmentId:null,modules} satisfies WorkContext;
 }
 const source=await normalOwner(f.owner),reviewer=await normalOwner(f.payer);
 const settings=await db.prepare('SELECT inventory_account_id,cash_account_id FROM commerce_settings WHERE company_id=?').bind(f.company).first<{inventory_account_id:string;cash_account_id:string}>();
 if(!settings)throw new Error('BROWSER_COMMERCE_ACCOUNT_PROFILE_REQUIRED');
 let inventoryId=settings.inventory_account_id;
 if(inventoryId===settings.cash_account_id||inventoryId===f.accounts.cash){
  if(await db.prepare('SELECT 1 FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND s.value_minor>0 LIMIT 1').bind(f.company).first())throw new Error('EXISTING_STOCK_MAPPING_RECONCILIATION_REQUIRED');
  const inventory=await createAccount(source,createAccountSchema.parse({chartId:f.chart,code:'BROWSER-INVENTORY',name:'Fictional business stock asset',category:'asset',currency:'THB'}),key());inventoryId=String(inventory.id);
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?').bind(inventoryId,f.company).run();
 }
 const accounting={inventory:inventoryId,wip:`${f.company}-browser-wip`,labor:`${f.company}-browser-labor`,overhead:`${f.company}-browser-overhead`};
 await db.batch((['wip','labor','overhead'] as const).map(name=>db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?)").bind(accounting[name],f.chart,f.company,`BROWSER-${name.toUpperCase()}`,`Fictional browser ${name}`,name==='wip'?'asset':'liability',now,now)));
 await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) SELECT ?,?,?,'5400','Declared browser stock count variance','expense','THB',?,? WHERE NOT EXISTS(SELECT 1 FROM finance_accounts WHERE chart_id=? AND company_id=? AND code='5400')").bind(`${f.company}-browser-count-variance`,f.chart,f.company,now,now,f.chart,f.company).run();
 const profile=await createManufacturingAccountingProfile(source,{chartId:f.chart,inventoryAccountId:accounting.inventory,wipAccountId:accounting.wip,laborAccountId:accounting.labor,overheadAccountId:accounting.overhead,scrapAccountId:f.accounts.expense},key());
 await reviewManufacturingAccountingProfile(f.manager,String(profile.id),{action:'approve',expectedVersion:1,reason:'Independent Finance review of the synthetic browser production adapter.'},key());
 async function routedJournal(input:Parameters<typeof createJournalSchema.parse>[0]){
  const journal=await createJournal(source,createJournalSchema.parse(input),key()),flow=await resolveFlow(db,source,'journal',journal.id);
  await assignFlowPrimary(db,source,'journal',journal.id,{stepKey:'draft',userId:f.manager.userId,expectedVersion:flow.version,recordVersion:1,reason:'Independent browser Finance journal review.',saveDefault:false},key());
  await postJournal(f.manager,journal.id,key(),{expectedVersion:1});return journal;
 }
 async function openingStock(warehouseId:string,productId:string,quantity:string,unitCost:string){
  const count=await saveCount(source,null,countSchema.parse({warehouseId,countDate:today,reason:'Declared fictional opening stock counted through the native controlled inventory command.',lines:[{productId,countedQuantity:quantity,unitCost}]}),key());
  await transitionCount(source,String(count.id),'submit',{expectedVersion:1,attachmentSetIds:[]},key());const flow=await resolveFlow(db,source,'stock_count',String(count.id));
  await assignFlowPrimary(db,source,'stock_count',String(count.id),{stepKey:'submitted',userId:f.manager.userId,expectedVersion:flow.version,recordVersion:2,reason:'Independent Finance checks initial counted valuation.',saveDefault:false},key());
  await transitionCount(f.manager,String(count.id),'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent review of the fictional counted opening quantity and cost.'},key());
  const approvedFlow=await resolveFlow(db,source,'stock_count',String(count.id));await assignFlowPrimary(db,source,'stock_count',String(count.id),{stepKey:'approved',userId:f.manager.userId,expectedVersion:approvedFlow.version,recordVersion:3,reason:'Finance posts the already independently approved opening valuation.',saveDefault:false},key());
  await transitionCount(f.manager,String(count.id),'post',{expectedVersion:3,attachmentSetIds:[],reason:'Post the independently approved fictional stock quantity and original value.'},key());return count.id;
 }
 const fixtures:Record<string,Record<string,unknown>>={};
 for(const viewport of ['desktop','mobile'] as const){
  const label=`${viewport} · Business engineering ${run}`;
  const project=await saveProject(db,source,null,projectSchema.parse({title:`${label} customer delivery`,description:'Fictional normal-login engineering workflow only.',memberUserIds:[reviewer.userId,f.manager.userId]}),key());
  const milestone=async(title:string,parent:string|null=null,depends:string[]=[],position=0)=>saveProjectMilestone(db,source,String(project.id),null,projectMilestoneSchema.parse({title,dueAt:new Date(now+7*86400000).toISOString(),ownerUserId:source.userId,parentMilestoneId:parent,dependsOnMilestoneIds:depends,ordinalPosition:position}),key());
  const root=await milestone('Customer delivery'),discovery=await milestone('Approved discovery',String(root.id),[],1),build=await milestone('Implemented release',String(root.id),[String(discovery.id)],2);
  const task=await saveTask(db,source,null,taskSchema.parse({title:`${label} implement customer work`,description:'Record source work and hand it back to the project owner.',assigneeUserId:reviewer.userId,projectId:project.id}),key());
  const costJournal=await routedJournal({chartId:f.chart,periodId:f.period,entryDate:today,currency:'THB',sourceType:'manual',memo:`${label} real project expense source`,lines:[{accountId:f.accounts.expense,debit:'75.01',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'75.01'}]});
  const cost=await createProjectCostAllocation(source,projectCostAllocationSchema.parse({projectId:project.id,sourceLineId:`${costJournal.id}-0`,amount:'75.01'}),key());
  await decideProjectCostAllocation(f.manager,cost.id,projectCostAllocationDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independent Finance approves the exact posted 75.01 THB source.'}),key());
  const asset=await createAsset(source,{branchId:f.branch,code:`BROWSER-${viewport}-${run}`,name:`${label} zero-cost safety station`,category:'Engineering inspection fixture',currency:'THB',cost:'0.00',acquiredDate:today},key());
  const preventive=await savePreventivePlan(source,null,{assetId:asset.id,assetVersion:1,title:`${label} daily safety inspection`,priority:'normal',ownerId:source.userId,intervalDays:1,firstDueAt:now-86400000,timeZone:'Asia/Bangkok',leadDays:0,reason:'Native draft definition awaits another normally authenticated reviewer.'},key());
  const warehouse=await saveMaster(source,'warehouses',null,{branchId:f.branch,code:`BROWSER-${viewport}-${run}`,name:`${label} controlled warehouse`},key());
  const material=await saveMaster(source,'products',null,{sku:`BROWSER-RAW-${viewport}-${run}`,name:`${label} raw material`,price:'5'},key()),output=await saveMaster(source,'products',null,{sku:`BROWSER-OUTPUT-${viewport}-${run}`,name:`${label} finished item`,price:'20'},key()),online=await saveMaster(source,'products',null,{sku:`BROWSER-ONLINE-${viewport}-${run}`,name:`${label} online item`,price:'20'},key());
  const spare=await saveMaster(source,'products',null,{sku:`BROWSER-SPARE-${viewport}-${run}`,name:`${label} replacement fuse`,price:'3.33'},key());
  const rawOpeningCount=await openingStock(String(warehouse.id),String(material.id),'10','5'),onlineOpeningCount=await openingStock(String(warehouse.id),String(online.id),'3','4'),spareOpeningCount=await openingStock(String(warehouse.id),String(spare.id),'3','3.33');
  const retailProduct=await saveMaster(source,'products',null,{sku:`BROWSER-RETAIL-${viewport}-${run}`,name:`${label} retail notebook`,price:'12.50',taxBps:0},key());
  const retailOpeningCount=await openingStock(String(warehouse.id),String(retailProduct.id),'3','3.33');
  const retailRegister=viewport==='desktop'
   ?await posCommand(db,source,'settings.bootstrap',{mode:'retail',warehouseId:String(warehouse.id),registerName:`${label} retail register`},key())
   :await posCommand(db,source,'register.save',{mode:'retail',warehouseId:String(warehouse.id),name:`${label} retail register`,expectedVersion:0},key());
  const retailRegisterId=String(retailRegister.registerId??retailRegister.id);
  await posCommand(db,source,'catalog.save',{productId:String(retailProduct.id),barcode:`RETAIL-${viewport}-${run}`,category:'Fictional retail',kind:'stock',station:'Retail counter',recipe:[],modifiers:[],taxInclusive:false,expectedVersion:0,registerIds:[retailRegisterId]},key());
  const resource=await saveProductionResource(reviewer,null,{branchId:f.branch,code:`BROWSER-${viewport}-${run}`,name:`${label} assembly cell`,dailyMinutes:20,workWeekdays:[0,1,2,3,4,5,6]},key());
  await saveProductionResourceCalendar(reviewer,String(resource.id),{expectedVersion:0,resourceVersion:1,workDate:today,availableMinutes:0,reason:'Fictional scheduled equipment shutdown on the initial planning day.'},key());
  const specification=await createSpecification(source,{name:`${label} routed assembly`,mode:'bom',outputProductId:output.id,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material.id,quantity:'2'}],operations:[{resourceId:resource.id,name:'Prepare and assemble',setupMinutes:5,runMinutesPerBatch:10},{resourceId:resource.id,name:'Package and inspect',setupMinutes:0,runMinutesPerBatch:5}]},key());
  await reviewSpecification(reviewer,String(specification.id),{action:'approve',expectedVersion:1,reason:'Independent normal-login reviewer approves the declared routed BOM.'},key());
  const customer=await saveMaster(source,'parties',null,{kind:'customer',code:`BROWSER-${viewport}-${run}`,name:`${label} fictional customer`,currency:'THB',creditLimit:'10000'},key());
  const tomorrow=new Date(Date.parse(`${today}T00:00:00Z`)+86400000).toISOString().slice(0,10),supplier=await saveMaster(source,'parties',null,{kind:'supplier',code:`BROWSER-SUPPLIER-${viewport}-${run}`,name:`${label} fictional supply partner`,currency:'THB'},key());
  async function confirmedOrder(kind:'sales'|'purchase'|'quote',partyId:string,productId:string,price:string){const order=await saveOrder(source,null,orderSchema.parse({kind,partyId,warehouseId:warehouse.id,orderDate:today,dueDate:tomorrow,lines:[{productId,quantity:'2',price,taxBps:0}]}),key());await transitionOrder(source,String(order.id),{action:'submit',expectedVersion:1,reason:'Fictional owner submits the actual source demand or incoming supply.'},key());const flow=await resolveFlow(db,source,'order',String(order.id));await assignFlowPrimary(db,source,'order',String(order.id),{stepKey:'submitted',userId:reviewer.userId,expectedVersion:flow.version,recordVersion:2,reason:'Independent normally authenticated source reviewer.',saveDefault:false},key());await transitionOrder(reviewer,String(order.id),{action:'approve',expectedVersion:2,reason:'Independent review confirms the actual upstream order source.'},key());return String(order.id);}
  const salesDemandId=await confirmedOrder('sales',String(customer.id),String(output.id),'20'),incomingSupplyId=await confirmedOrder('purchase',String(supplier.id),String(material.id),'5');
  const portalOrderId=await confirmedOrder('quote',String(customer.id),String(online.id),'10'),portalUser=viewport==='desktop'?f.claimant:f.head,unrelatedUser=viewport==='desktop'?f.head:f.claimant;
  const portalEmail=await db.prepare('SELECT email FROM hr_people WHERE company_id=? AND user_id=?').bind(f.company,portalUser.userId).first<string>('email');
  if(!portalEmail)throw new Error('NORMAL_VERIFIED_CUSTOMER_BROWSER_IDENTITY_REQUIRED');
  const portalBinding=await requestCustomerAccess(source,{orderId:portalOrderId,sourceVersion:3,email:portalEmail,expiresAt:now+7*86400000,reason:'Fictional customer controls the normal verified account; independent Sales authority review is required before this one quotation can be shared.'},key());
  const channel=await createChannel(source,{name:`${label} web channel`,channelType:'web',configuration:{storefront:'Declared sandbox only'}},key());
  await changeEcommerce(reviewer,'channel',String(channel.id),{action:'activate',expectedVersion:1,reason:'Independent review of the sandbox channel.'},key());
  const listing=await createListing(source,{channelId:channel.id,productId:online.id,title:`${label} published online item`,currency:'THB',price:'20.01'},key());
  await changeEcommerce(reviewer,'listing',String(listing.id),{action:'publish',expectedVersion:1,reason:'Independent review of the exact published price snapshot.'},key());
  const cart=await createCart(source,{channelId:channel.id,partyId:customer.id,warehouseId:warehouse.id,currency:'THB'},key());
  await addCartLine(source,{cartId:cart.id,expectedVersion:1,listingId:listing.id,listingVersion:2,quantity:'1'},key());
  const serviceSource=await saveOrder(source,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:warehouse.id,orderDate:today,dueDate:today,lines:[{productId:online.id,quantity:'1',price:'20.01',taxBps:0}]}),key());
  const ticket=await createSupportTicket(source,{orderId:serviceSource.id,sourceVersion:1,title:`${label} on-site customer inspection`,issue:'Fictional installed device needs an on-site functional inspection.',ownerId:f.claimant.userId,dueDate:today},key());
  const agreement=await createAgreement(source,{branchId:f.branch,customerId:customer.id,title:`${label} on-site inspection agreement`,startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:10000},key());
  await changeAgreement(reviewer,String(agreement.id),{action:'activate',expectedVersion:1,reason:'Independent reviewer approves the chargeable fictional visit.'},key());
  const service=await createServiceOrder(source,{ticketId:ticket.id,ticketVersion:1,agreementId:agreement.id,agreementVersion:2,issue:'Inspect the fictional installed device and obtain customer acknowledgement.',site:'Declared engineering sandbox customer site',technicianId:source.userId,scheduledAt:now+60000},key());
  const bankKey=key();await importBankStatement(source,{bankAccount:settings.cash_account_id,statementNumber:`BROWSER-FIELD-${viewport}-${run}`,statementDate:today,opening:'0',closing:'100',sourceHash:`declared-synthetic-browser-bank-${viewport}-${run}`,rows:[{externalId:`BROWSER-FIELD-COLLECT-${viewport}-${run}`,transactionDate:today,description:'Declared synthetic bank provider result for the fictional customer visit.',amount:'100',currency:'THB'}]},bankKey);
  fixtures[viewport]={project:{id:project.id,rootId:root.id,discoveryId:discovery.id,buildId:build.id,taskId:task.id,costId:cost.id,costJournalId:costJournal.id,costMinor:7501,recipientId:reviewer.userId},preventive:{assetId:asset.id,id:preventive.id,firstDueAt:now-86400000,warehouseId:warehouse.id,spareId:spare.id,openingCountId:spareOpeningCount,initialValueMinor:999},production:{warehouseId:warehouse.id,materialId:material.id,outputId:output.id,specificationId:specification.id,resourceId:resource.id,openingCountId:rawOpeningCount,initialQuantityMilli:10000,initialValueMinor:5000,salesDemandId,incomingSupplyId},ecommerce:{channelId:channel.id,listingId:listing.id,cartId:cart.id,warehouseId:warehouse.id,productId:online.id,partyId:customer.id,openingCountId:onlineOpeningCount,initialQuantityMilli:3000,initialValueMinor:1200,amountMinor:2001},fieldService:{id:service.id,ticketId:ticket.id,agreementId:agreement.id,amountMinor:10000,bankAccountId:settings.cash_account_id,bankTransactionId:`bankrow-${bankKey}-0`},retail:{registerId:retailRegisterId,registerName:`${label} retail register`,warehouseId:warehouse.id,productId:retailProduct.id,productName:`${label} retail notebook`,openingCountId:retailOpeningCount,initialQuantityMilli:3000,initialValueMinor:999,priceMinor:1250},customerAuthority:{bindingId:portalBinding.id,orderId:portalOrderId,excludedOrderId:salesDemandId,recipientUserId:portalUser.userId,recipientEmail:portalEmail,unrelatedUserId:unrelatedUser.userId}};
 }
 return {synthetic:true,environment:'Disposable PostgreSQL browser workspace; normal credential authentication and assignments',companyId:f.company,branchId:f.branch,chartId:f.chart,periodId:f.period,today,accounting,personas:{sourceUserId:source.userId,reviewerUserId:reviewer.userId,financeUserId:f.manager.userId,inspectorUserId:f.reviewer.userId},viewports:fixtures};
}
