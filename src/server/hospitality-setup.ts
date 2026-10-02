import 'server-only';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import { all, one, str } from './pos-core';
import { hasPermission } from '@/lib/permissions';
import { ApiError } from './http';
import { loadPms } from './pms';
import { readiness } from '@/lib/pms-engine';
import { pmsAccountKeys, records as pmsRecords, type Kind } from '@/lib/pms-model';
import { pmsAccountCategory, pmsAccountCode } from './hospitality-accounting';

export async function hospitalitySetup(actor: WorkContext, branchId?: string, propertyId?: string) {
  const can = (p:string) => hasPermission(actor,p);
  if(actor.beta || actor.departmentId || !['pms.configure','pos.configure','pms.accounting.prepare','pms.accounting.approve'].some(can))
    throw new ApiError(403,'PERMISSION_REQUIRED','Hotel, restaurant or Finance setup access is required.');
  const {write:db}=await holdingResources(actor.holdingId),company=actor.companyId;
  const branches=await all(db,"SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name",company,actor.branchId,actor.branchId);
  const branch=actor.branchId??branchId??str(branches[0]??{},'id');
  if(branch&&!branches.some(r=>r.id===branch))throw new ApiError(404,'BRANCH_NOT_FOUND','Choose an accessible company branch.');
  const properties=await all(db,'SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name',company,branch);
  const property=propertyId?properties.find(r=>r.id===propertyId):properties[0];
  if(propertyId&&!property)throw new ApiError(404,'PMS_NOT_FOUND','Hotel not found in this branch.');
  const finance=can('pms.accounting.prepare')||can('pms.accounting.approve');
  const pms=can('pms.configure')||finance;
  const snapshot=pms&&property?await loadPms(db,actor,String(property.id)):null;
  const date=snapshot?.property.businessDate??new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date());
  const [commerce,settings,warehouses,registers,roles,periods,charts,accounts,products,tables,stock]=await Promise.all([
    one(db,'SELECT s.currency,s.chart_id,c.base_currency chart_currency,c.status chart_status FROM commerce_settings s LEFT JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?',company),
    one(db,'SELECT enabled,currency FROM pos_settings WHERE company_id=?',company),
    all(db,"SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",company,branch),
    all(db,"SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=? AND branch_id=? AND active=true AND mode='restaurant'",company,branch),
    all(db,"SELECT DISTINCT role,user_id FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))",company,Date.now(),Date.now(),branch),
    all(db,"SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?",company,date,date),
    finance?all(db,"SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active'",company):Promise.resolve([]),
    finance?all(db,"SELECT id,code,name,category,currency,chart_id FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code",company):Promise.resolve([]),
    can('pos.configure')?all(db,"SELECT p.id,p.name,p.sku,p.unit,p.price_minor,p.tax_bps,p.version,c.kind,c.station,c.recipe_json,COALESCE(c.version,0) catalog_version FROM commerce_products p LEFT JOIN pos_catalog c ON c.product_id=p.id WHERE p.company_id=? AND p.status='active' ORDER BY p.name",company):Promise.resolve([]),
    can('pos.configure')?all(db,'SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND r.branch_id=?',company,branch):Promise.resolve([]),
    one(db,'SELECT COUNT(*) n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND w.branch_id=? AND s.quantity_milli>0',company,branch),
  ]);
  const roleNeeds=[['Hotel administrator',['pms_manager','holding_owner']],['Front desk',['pms_frontdesk','pms_manager','holding_owner']],['Hotel cashier',['pms_cashier','holding_owner']],['Restaurant cashier',['pos_cashier','pos_manager','holding_owner']],['Kitchen',['pos_kitchen','holding_owner']],['Housekeeping',['pms_housekeeping','holding_owner']],['Room inspector',['pms_supervisor','holding_owner']],['Finance preparer',['finance_preparer','holding_owner']],['Finance approver',['finance_approver','holding_owner']]] as const;
  const missingRoles=roleNeeds.filter(([,valid])=>!roles.some(r=>(valid as readonly string[]).includes(String(r.role)))).map(([label])=>label);
  const preparers=roles.filter(r=>['finance_preparer','holding_owner'].includes(String(r.role)));
  const independent=roles.some(r=>['finance_approver','holding_owner'].includes(String(r.role))&&preparers.some(p=>p.user_id!==r.user_id));
  const records=snapshot?.records??[],rows=<K extends Kind>(kind:K)=>snapshot?pmsRecords(snapshot,kind):[];
  const hotelIssues=snapshot?readiness(snapshot):['Create the hotel'];
  const steps=[
    {id:'location',title:'Choose location',owner:'Hotel administrator',href:'/people/organization',blockers:branch?[]:['Add an active hotel branch']},
    {id:'finance',title:'Prepare Finance and stock',owner:'Finance preparer',href:'/finance/setup',blockers:[...(!commerce?['Configure Commerce accounts']:commerce.currency!=='THB'||commerce.chart_currency!=='THB'||commerce.chart_status!=='active'?['Finance must review a THB Commerce chart']:[]),...(periods.length!==1?['Open exactly one accounting period for the business date']:[]),...(!warehouses.length?['Add a kitchen warehouse']:[])]},
    {id:'hotel',title:'Configure the hotel',owner:'Hotel administrator',href:'/pms/settings',blockers:pms?[...(!snapshot?['Create the hotel']:[]),...(!rows('room_type').length?['Add room types']:[]),...(!rows('room').some(r=>r.data.active)?['Add active rooms']:[]),...(rows('room_type').some(t=>!rows('rate').some(r=>r.data.typeId===t.id&&r.data.active&&r.data.from<=date&&r.data.to>=date&&r.data.price>0))?['Enter a current nightly price for each room type']:[]),...(!snapshot?.property.config.cancellationPolicy?['Enter a cancellation policy']:[])]:['Hotel setup requires hotel access']},
    {id:'restaurant',title:'Configure the restaurant',owner:'Restaurant manager',href:'/pos?tab=settings',blockers:[...(!settings?.enabled?['Configure and enable POS']:settings.currency!=='THB'?['Align POS currency with the hotel']:[]),...(!registers.length?['Add a restaurant register in this branch']:[]),...(can('pos.configure')? [...(!tables.length?['Add restaurant tables']:[]),...(!products.some(r=>r.kind&&Number(r.price_minor)>0)||products.some(r=>r.kind&&Number(r.price_minor)<=0)?['Add a priced menu and recipes']:[])]:['Menu review requires restaurant setup access']),...(Number(stock?.n??0)===0?['Approve opening ingredient stock']:[])]},
    {id:'team',title:'Prepare the team',owner:'Access administrator',href:'/people/settings',blockers:[...missingRoles,...(!independent?['Assign different people to Finance preparation and approval']:[])]},
    {id:'open',title:'Review and open',owner:'Finance approver / Hotel administrator',href:'/pms/settings',blockers:[...hotelIssues,...(snapshot?.property.state!=='open'?['Verify readiness and open the hotel']:[]),...(rows('room').some(r=>r.data.active&&r.data.cleanliness!=='inspected')?['Housekeeping must complete and independently inspect rooms']:[])]},
  ];
  return {companyId:company,companyName:str((await one(db,'SELECT display_name FROM organization_nodes WHERE id=?',company))??{},'display_name'),branchId:branch,branches,properties,propertyId:property?.id??null,date,
    permissions:{hotel:can('pms.configure'),restaurant:can('pos.configure'),productCreate:can('inventory.prepare'),financePrepare:can('pms.accounting.prepare')&&can('finance.write'),financeApprove:can('pms.accounting.approve')},
    steps,ready:steps.every(s=>!s.blockers.length),hotel:snapshot?{property:{...snapshot.property,config:{...snapshot.property.config,...(!finance?{accounts:Object.fromEntries(pmsAccountKeys.map(k=>[k,''])) as typeof snapshot.property.config.accounts,chartId:''}:{})}},records:records.filter(r=>['room','room_type','rate'].includes(r.kind))}:null,
    posConfigured:Boolean(settings),posCurrency:settings?.currency??null,warehouses,registers,products,tables,charts,accounts,
    suggestions:finance?pmsAccountKeys.map(key=>({key,code:pmsAccountCode(key),category:pmsAccountCategory(key)})):[],
    roomChargeRequirements:['Same company and branch','Matching THB currency and business date','Checked-in stay with an open, uninvoiced folio','All kitchen items delivered','Entire bill unpaid'],
    activationRestrictions:process.env.NODE_ENV==='production'?['Production release gates apply',...(process.env.ERPX_POS_ROOM_CHARGE_PILOT_ENABLED!=='true'?['Restaurant room charging requires pilot activation']:[])]:['Rehearsal readiness does not approve production activation'],
  };
}
export type HospitalitySetup = Awaited<ReturnType<typeof hospitalitySetup>>;
