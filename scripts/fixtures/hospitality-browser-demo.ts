/** Real PMS/POS setup inside the disposable browser simulation databases only. */
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { PostgresDatabase } from "../../src/server/runtime/postgres";
import type { SqlDatabase } from "../../src/server/runtime/types";
import type { WorkContext } from "../../src/server/authorization";
import { businessRoles } from "../../src/lib/business-roles";
import { accountKeys } from "../../src/lib/pos";
import { pmsAccountKeys, plusDays, record, records, type Config } from "../../src/lib/pms";

type Fixture = {
  name:string;legalEntity:string;property:{rooms:{number:string;type:string;state:string}[];roomTypes:{code:string;name:string;capacity:number;rateMinor:number}[];taxAndService:{vatBpsExampleOnly:number;serviceBpsExampleOnly:number;taxInclusiveExampleOnly:boolean}};
  restaurant:{register:string;warehouse:string;tables:string[];menu:{sku:string;name:string;priceMinor:number;recipe?:{sku:string;quantityMilli:number}[];stockSku?:string}[];openingStock:{sku:string;unit:string;quantityMilli:number;valueMinor:number}[]};
  demoActors:{key:string;role:keyof typeof businessRoles|"holding_owner";email:string}[];
};
type DemoUser={label:string;email:string;role:string;userId:string;roles:string[]};

export async function seedHospitalityBrowserDemo(input:{db:SqlDatabase;control:SqlDatabase;files:Parameters<typeof import("../../fixtures/expenses/company").seedExpenseCompany>[1];passwordHash:string;run:string;origin:string;menuImages?:boolean}) {
  if(!(input.db instanceof PostgresDatabase))throw new Error("ISOLATED_POSTGRES_REQUIRED");
  const db=input.db;
  const fixture=JSON.parse(await readFile(resolve(import.meta.dir,"../../fixtures/hospitality/pilot-example.json"),"utf8")) as Fixture;
  const {seedExpenseCompany}=await import("../../fixtures/expenses/company");
  const {pmsCommand,loadPms}=await import("../../src/server/pms");
  const {posCommand}=await import("../../src/server/pos-core");
  const {saveMaster}=await import("../../src/server/commerce");
  const {saveServiceTeam,serviceTeamSchema}=await import("../../src/server/service-teams");
  const {saveSupplierInvoicePolicy}=await import("../../src/server/supplier-bill-matching");
  const f=await seedExpenseCompany(input.db,input.files);
  await input.db.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?").bind(fixture.legalEntity,f.company).run();
  await input.db.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?").bind("Riverside Lantern Bangkok",f.branch).run();
  const now=Date.now(),actors:Record<string,WorkContext>={},users:DemoUser[]=[];
  for(const entry of fixture.demoActors) {
    const actor=entry.role==="holding_owner"&&entry.key==="administrator"?f.owner:await f.member(`riverside-${entry.key}`,entry.role);
    actor.holdingId="h002";actor.holdingSlug="riverside-lantern";actor.holdingName=fixture.name;actor.scope={kind:"holding",holdingId:"h002",holdingSlug:"riverside-lantern",holdingName:fixture.name,companyId:f.company};
    actor.email=entry.email;actor.name=entry.key.replace(/([A-Z])/g," $1").replace(/^./,c=>c.toUpperCase());
    const modules=entry.role==="holding_owner"?["pms","pos","finance","inventory","procurement","organization","iam"]:businessRoles[entry.role];
    actor.modules=[...modules];
    await input.db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(modules),actor.assignmentId).run();
    await input.db.prepare("UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?").bind(actor.name,actor.name,entry.email,f.company,actor.userId).run();
    await input.control.batch([
      input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,entry.email,now,now),
      input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,input.passwordHash,now,now),
      input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,actor.name,now),
    ]);
    actors[entry.key]=actor;users.push({label:actor.name,email:entry.email,role:entry.role,userId:actor.userId,roles:[entry.role]});
  }
  // Expense approval requires an explicit numbered organization, active
  // positions, and reporting lines. Keep the synthetic H002 actors valid so
  // the production route can exercise independent department and Finance
  // review in this disposable database.
  const orgLevels=Array.from({length:6},(_,index)=>`${f.company}-riverside-level-${index+1}`);
  await db.batch(orgLevels.map((id,index)=>db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING")
    .bind(id,f.company,`RIVERSIDE_L${index+1}`,`Riverside Level ${index+1}`,-(index+1),index+1,now,now)));
  const orgPositions={ceo:`${f.company}-riverside-position-ceo`,manager:`${f.company}-riverside-position-manager`,financeApprover:`${f.company}-riverside-position-finance-approver`,staff:`${f.company}-riverside-position-staff`,employee:`${f.company}-riverside-position-employee`,frontDesk:`${f.company}-riverside-position-front-desk`};
  const positionRows=[
    {id:orgPositions.ceo,title:"Riverside holding lead",level:1,reportsTo:null,isCeo:1},
    {id:orgPositions.manager,title:"Riverside department manager",level:3,reportsTo:orgPositions.ceo,isCeo:0},
    {id:orgPositions.financeApprover,title:"Riverside Finance approver",level:4,reportsTo:orgPositions.manager,isCeo:0},
    {id:orgPositions.staff,title:"Riverside hospitality specialist",level:5,reportsTo:orgPositions.manager,isCeo:0},
    {id:orgPositions.employee,title:"Riverside hospitality employee",level:6,reportsTo:orgPositions.manager,isCeo:0},
    {id:orgPositions.frontDesk,title:"Riverside front desk",level:5,reportsTo:orgPositions.manager,isCeo:0},
  ];
  await db.batch(positionRows.map(position=>db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,?,?,?,?,?,1,1,?,?,?) ON CONFLICT DO NOTHING")
    .bind(position.id,f.company,position.title,orgLevels[position.level-1],position.reportsTo,position.isCeo,position.id===orgPositions.frontDesk?1:0,actors.administrator.userId,now,now)));
  const managerPersonId=`person-${actors.peopleManager.userId}`;
  const organizationAssignments=Object.entries(actors).map(([key,actor])=>{
    const isOwner=key==="administrator",isManager=key==="peopleManager",isFinanceApprover=key==="financeApprover",isEmployee=key==="employee"||key==="foreignEmployee"||key==="dailyEmployee"||key==="hourlyEmployee";
    const level=isOwner?1:isManager?3:isFinanceApprover?4:isEmployee?6:5;
    const position=isOwner?orgPositions.ceo:isManager?orgPositions.manager:isFinanceApprover?orgPositions.financeApprover:key==='employee'?orgPositions.frontDesk:isEmployee?orgPositions.employee:orgPositions.staff;
    const manager=isOwner?null:isManager?`person-${actors.administrator.userId}`:managerPersonId;
    const branch=isOwner?null:f.branch,department=isOwner?null:f.department;
    return db.prepare("UPDATE hr_engagements e SET branch_id=?,department_id=?,level_id=?,position_id=?,manager_person_id=?,version=e.version+1,updated_at=? FROM hr_people p WHERE e.person_id=p.id AND e.company_id=? AND p.user_id=? AND e.status='active'")
      .bind(branch,department,orgLevels[level-1],position,manager,now,f.company,actor.userId);
  });
  await db.batch(organizationAssignments);
  const admin=actors.administrator,finance=actors.financeApprover;
  // Give the isolated hotel company explicit independent department and HR
  // authority so request, overtime, and leave scenarios exercise a usable
  // review route instead of the real setup-blocker path.
  const {changeAppointment}=await import("../../src/server/department-authority");
  const appointmentVersion=async(kind:"head"|"hr",scopeId:string)=>Number((await db.prepare("SELECT version FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?")
    .bind(f.company,kind,scopeId).first<{version:number}>())?.version??0);
  await changeAppointment(db,admin,{kind:"head",departmentId:f.department,positionId:null,personId:managerPersonId,expectedVersion:await appointmentVersion("head",f.department),reason:"Synthetic department approval route for H002 browser acceptance."},crypto.randomUUID());
  await changeAppointment(db,admin,{kind:"hr",departmentId:null,positionId:null,personId:`person-${actors.hrAdmin.userId}`,expectedVersion:await appointmentVersion("hr",f.company),reason:"Synthetic independent HR route for H002 browser acceptance."},crypto.randomUUID());
  await saveServiceTeam(db,admin,null,serviceTeamSchema.parse({
    name:"Riverside purchasing team",
    members:[
      {userId:actors.purchasingOperator.userId,responsibility:"lead"},
      {userId:actors.purchasingManager.userId,responsibility:"member"},
      {userId:actors.purchasingReviewer.userId,responsibility:"member"},
    ],
    coverage:[{category:"purchasing",scopeKind:"holding",scopeNodeId:f.company}],
  }),crypto.randomUUID());
  await saveSupplierInvoicePolicy(admin,{matchingMode:"required",expectedVersion:0,reason:"Fictional Riverside scenario requires independent matching of supplier invoices to receipts."},crypto.randomUUID());
  const supplierOne=await saveMaster(admin,"parties",null,{kind:"supplier",code:"RIVERSIDE-FOOD-01",name:"Fictional Riverside Food Supply",paymentTermsDays:30},crypto.randomUUID());
  const supplierTwo=await saveMaster(admin,"parties",null,{kind:"supplier",code:"RIVERSIDE-FOOD-02",name:"Fictional Lantern Produce Co",paymentTermsDays:30},crypto.randomUUID());
  const {saveEmployeeRequest,employeeRequestSchema}=await import("../../src/server/employee-requests");
  const purchaseRequest=await saveEmployeeRequest(db,actors.employee,null,employeeRequestSchema.parse({
    kind:"purchase",title:"Riverside kitchen rice restock",description:"Fictional dry stock for the restaurant kitchen.",
    location:"Riverside Lantern kitchen",impact:"normal",branchId:f.branch,departmentId:f.department,
    neededDate:"2026-10-15",currency:"THB",items:[{description:"Rice for restaurant service",quantity:"10",unit:"kg",estimatedCost:"12.50"}],
  }),crypto.randomUUID());
  const payrollGroupId=`${f.company}-riverside-mock-monthly`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,?,'TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,?,?)")
    .bind(payrollGroupId,f.company,"Riverside mock monthly · THB",admin.userId,now,now).run();
  const payrollManager=await db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=?").bind(f.company,actors.peopleManager.userId).first<{id:string}>();
  const payrollFixtures=[
    {key:"employee",employmentType:"salaried",startDate:"2020-01-01",country:"TH",payFrequency:"monthly",baseMinor:100000,hourlyMinor:0,expectedHours:160,components:[
      {code:"MOCK_FIXTURE:bonus",label:"Fictional bonus fixture",kind:"earning",amount:"50.00"},
      {code:"MOCK_FIXTURE:service-charge",label:"Fictional service-charge fixture",kind:"earning",amount:"12.50"},
      {code:"MOCK_FIXTURE:withholding",label:"Fictional withholding fixture",kind:"deduction",amount:"15.00"},
      {code:"MOCK_FIXTURE:employer-contribution",label:"Fictional employer-cost fixture",kind:"employer",amount:"5.00"},
      {code:"MOCK_FIXTURE:YTD_CONTEXT",label:"Year-to-date context fixture only",kind:"ytd_context",amount:"250.00"},
    ]},
    {key:"foreignEmployee",employmentType:"salaried",startDate:"2026-09-16",country:"JP",payFrequency:"monthly",baseMinor:100000,hourlyMinor:0,expectedHours:160,components:[
      {code:"MOCK_FIXTURE:foreign-proration-withholding",label:"Fictional foreign-worker withholding fixture",kind:"deduction",amount:"10.00"},
    ]},
    {key:"dailyEmployee",employmentType:"shift",startDate:"2020-01-01",country:"TH",payFrequency:"off_cycle",baseMinor:0,hourlyMinor:5000,expectedHours:8,components:[
      {code:"MOCK_FIXTURE:daily-withholding",label:"Fictional daily-worker withholding fixture",kind:"deduction",amount:"1.00"},
    ]},
    {key:"hourlyEmployee",employmentType:"hourly",startDate:"2020-01-01",country:"TH",payFrequency:"monthly",baseMinor:0,hourlyMinor:2500,expectedHours:160,components:[
      {code:"MOCK_FIXTURE:hourly-withholding",label:"Fictional hourly-worker withholding fixture",kind:"deduction",amount:"2.50"},
    ]},
  ] as const;
  const payrollEngagementIds:Record<string,string>={};
  const payrollPersonIds:Record<string,string>={};
  for(const fixture of payrollFixtures) {
    const actor=actors[fixture.key];
    const person=await db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=?").bind(f.company,actor.userId).first<{id:string}>();
    const engagement=await db.prepare("SELECT id FROM hr_engagements WHERE company_id=? AND person_id=? AND status='active' ORDER BY start_date LIMIT 1").bind(f.company,person!.id).first<{id:string}>();
    if(!person||!engagement)throw new Error(`RIVERSIDE_PAYROLL_FIXTURE_MISSING:${fixture.key}`);
    await db.prepare("UPDATE hr_people SET country_code=?,preferred_name=legal_name,updated_at=? WHERE id=?").bind(fixture.country,now,person.id).run();
    await db.prepare("UPDATE hr_engagements SET employment_type=?,start_date=?,pay_group_id=?,manager_person_id=?,currency='THB',version=version+1,updated_at=? WHERE id=?")
      .bind(fixture.employmentType,fixture.startDate,payrollGroupId,payrollManager?.id??null,now,engagement.id).run();
    const compensationId=`${engagement.id}-riverside-mock-compensation`;
    await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01',?,?,?,?,'THB',?,'approved',?,?,?)")
      .bind(compensationId,engagement.id,fixture.payFrequency,fixture.baseMinor,fixture.hourlyMinor,fixture.expectedHours,JSON.stringify(fixture.components),actors.payrollPreparer.userId,now,now).run();
    payrollEngagementIds[fixture.key]=engagement.id;
    payrollPersonIds[fixture.key]=person.id;
  }
  const timeEntries=[
    {key:"dailyEmployee",date:"2026-09-10",regular:8,overtime:0},
    {key:"hourlyEmployee",date:"2026-09-10",regular:8,overtime:2},
    {key:"hourlyEmployee",date:"2026-09-11",regular:8,overtime:0},
    {key:"hourlyEmployee",date:"2026-09-12",regular:8,overtime:0},
    {key:"hourlyEmployee",date:"2026-09-13",regular:8,overtime:0},
  ];
  for(const [index,entry] of timeEntries.entries()) {
    const engagementId=payrollEngagementIds[entry.key];
    await db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,status,note,version,submitted_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'approved',?,1,?,?,?,?)")
      .bind(`${f.company}-riverside-mock-time-${index+1}`,engagementId,f.company,entry.date,entry.regular,entry.overtime,"Fictional approved time fixture",actors[entry.key].userId,actors.peopleManager.userId,now,now).run();
  }
  const leaveTypeId=`${f.company}-riverside-mock-annual-leave`;
  const leavePolicyId=`${f.company}-riverside-mock-leave-policy-v1`;
  const leaveAssignmentId=`${f.company}-riverside-mock-leave-assignment`;
  const employeeEngagementId=payrollEngagementIds.employee;
  await db.batch([
    db.prepare("INSERT INTO leave_types(id,company_id,code,name,description,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?, 'MOCK-ANNUAL','Synthetic annual leave','Simulation-only balance and working-day fixture.',true,'2026-01-01',?,?,?)").bind(leaveTypeId,f.company,admin.userId,now,now),
    db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,allow_negative,created_by,created_at) VALUES(?,?, 'riverside-mock',1,'Riverside mock leave policy','active','2026-01-01','Asia/Bangkok','[1,2,3,4,5]'::jsonb,false,?,?)").bind(leavePolicyId,f.company,admin.userId,now),
    db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,employee_eligible,contractor_eligible) VALUES(?,?,'annual',10,0,true,false)").bind(leavePolicyId,leaveTypeId),
    db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?, '2026-01-01',?,?)").bind(leaveAssignmentId,f.company,employeeEngagementId,leavePolicyId,admin.userId,now),
    db.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at) VALUES(?,?,?,?,'opening',10,'2026-01-01',2026,?,'Synthetic opening allowance for mock E2E only',?,?)").bind(`${leaveAssignmentId}-opening`,f.company,employeeEngagementId,leaveTypeId,`${leaveAssignmentId}-opening`,admin.userId,now),
  ]);
  const rosterWeekId=`${f.company}-riverside-mock-roster-week`;
  const rosterShiftId=`${f.company}-riverside-mock-roster-shift`;
  const rosterDate="2026-09-29";
  const nextRosterDate="2026-10-06";
  const nextRosterWeekId=`${rosterWeekId}-next`;
  const nextRosterShiftId=`${rosterShiftId}-next`;
  await db.batch([
    db.prepare("INSERT INTO people_roster_locks(company_id,version) VALUES(?,1)").bind(f.company),
    db.prepare("INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,state,version,created_by,published_by,created_at,updated_at) VALUES(?,?,?,?, '2026-09-28','Asia/Bangkok','published',1,?,?,?,?)").bind(rosterWeekId,f.company,f.branch,f.department,actors.peopleManager.userId,actors.peopleManager.userId,now,now),
    db.prepare("INSERT INTO people_roster_shifts(id,week_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,state,version) VALUES(?,?, 'Fictional front desk coverage',?,'09:00','17:00',?,?,1,'active',1)").bind(rosterShiftId,rosterWeekId,rosterDate,Date.parse(`${rosterDate}T09:00:00+07:00`),Date.parse(`${rosterDate}T17:00:00+07:00`)),
    db.prepare("INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id,state,version) VALUES(?,?,?,?,'assigned',1)").bind(`${rosterShiftId}-employee`,rosterShiftId,payrollPersonIds.employee,employeeEngagementId),
    db.prepare("INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,state,version,created_by,published_by,created_at,updated_at) VALUES(?,?,?,?, '2026-10-05','Asia/Bangkok','published',1,?,?,?,?)").bind(nextRosterWeekId,f.company,f.branch,f.department,actors.peopleManager.userId,actors.peopleManager.userId,now,now),
    db.prepare("INSERT INTO people_roster_shifts(id,week_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,state,version) VALUES(?,?, 'Fictional front desk coverage',?,'09:00','17:00',?,?,1,'active',1)").bind(nextRosterShiftId,nextRosterWeekId,nextRosterDate,Date.parse(`${nextRosterDate}T09:00:00+07:00`),Date.parse(`${nextRosterDate}T17:00:00+07:00`)),
    db.prepare("INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id,state,version) VALUES(?,?,?,?,'assigned',1)").bind(`${nextRosterShiftId}-employee`,nextRosterShiftId,payrollPersonIds.employee,employeeEngagementId),
  ]);
  for(const [stepKey,userId,reason] of [
    ["calculate",actors.payrollPreparer.userId,"Synthetic hotel payroll preparation owner"],
    ["preview",actors.payrollPreparer.userId,"Synthetic hotel payroll review submission owner"],
    ["in_review",actors.payrollApprover.userId,"Independent synthetic hotel payroll reviewer"],
    ["approved",finance.userId,"Synthetic hotel Finance posting owner"],
    ["posted",actors.treasury.userId,"Synthetic hotel payment preparation owner"],
    ["payment_draft",finance.userId,"Independent synthetic hotel payment approver"],
    ["payment_approved",actors.treasury.userId,"Synthetic hotel payment export owner"],
    ["payment_exported",actors.treasury.userId,"Synthetic hotel payment confirmation owner"],
    ["payment_partially_settled",actors.treasury.userId,"Synthetic hotel payment retry owner"],
    ["payment_settled",actors.treasury.userId,"Synthetic hotel payment settlement owner"],
  ] as const) {
    await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll',?,?,?,?,?)")
      .bind(f.company,stepKey,userId,admin.userId,reason,now).run();
  }
  const pmsAccounts={} as Config["accounts"];
  const allKeys=[...new Set<string>([...pmsAccountKeys,...accountKeys])];
  for(const [index,name] of allKeys.entries()) {
    const id=`${f.company}-hospitality-${name}`;
    const category=["revenue","service"].includes(name)?"revenue":["advance","deposit","tax","outputTax","refund","disputes"].includes(name)?"liability":["consumption","commission","cashDifference","cogs","fees","waste"].includes(name)?"expense":"asset";
    await input.db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?)").bind(id,f.chart,f.company,`HOTEL-${index}`,`Riverside ${name}`,category,now,now).run();
    if((pmsAccountKeys as readonly string[]).includes(name))pmsAccounts[name as keyof Config["accounts"]]=id;
  }
  const posAccounts=Object.fromEntries(accountKeys.map(name=>[name,`${f.company}-hospitality-${name}`]));
  const date=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date());
  const nextYear=`${Number(date.slice(0,4))+1}-${date.slice(5)}`;
  const propertyId=String((await pmsCommand(input.db,admin,{operation:"property.create",input:{name:fixture.name,branchId:f.branch,businessDate:date}},crypto.randomUUID())).id);
  const pms=async(operation:string,details:unknown={},actor=admin)=>{
    const snapshot=await loadPms(input.db,admin,propertyId);
    return pmsCommand(db,actor,{propertyId,expectedVersion:snapshot.property.version,operation,input:details},crypto.randomUUID());
  };
  await pms("accounting.save",{chartId:f.chart,accounts:pmsAccounts,taxBps:fixture.property.taxAndService.vatBpsExampleOnly,serviceBps:fixture.property.taxAndService.serviceBpsExampleOnly,taxInclusive:fixture.property.taxAndService.taxInclusiveExampleOnly,advanceTax:true});
  await pms("accounting.approve",{},finance);
  // Shared physical inventory uses the independently reviewed hotel account.
  // The generic expense fixture's cash account is not an inventory valuation map.
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?').bind(pmsAccounts.inventory,f.company).run();
  const types:Record<string,string>={};
  for(const roomType of fixture.property.roomTypes)types[roomType.name]=String((await pms("room_type.save",{name:roomType.name,capacity:roomType.capacity,amenities:"Fictional demo room"})).id);
  const rooms:Record<string,string>={};
  for(const room of fixture.property.rooms)rooms[room.number]=String((await pms("room.save",{number:room.number,typeId:types[room.type],active:room.state!=="maintenance_blocked"})).id);
  const rates:Record<string,string>={};
  for(const roomType of fixture.property.roomTypes)rates[roomType.name]=String((await pms("rate.save",{name:`${roomType.name} flexible`,typeId:types[roomType.name],from:date,to:nextYear,price:roomType.rateMinor})).id);
  await pms("property.enable");
  for(const room of fixture.property.rooms.filter(room=>room.state==="inspected"||room.state==="occupied")) {
    const taskId=String((await pms("task.create",{roomId:rooms[room.number],type:"arrival",date})).id);
    const task=record(await loadPms(input.db,admin,propertyId),"task",taskId);
    await pms("task.complete",{id:taskId,checked:task.data.checklist},actors.housekeeper);
    await pms("task.inspect",{id:taskId},actors.inspector);
  }
  const occupied=fixture.property.rooms.find(room=>room.state==="occupied");
  if(occupied) {
    const guestId=String((await pms("guest.save",{name:"Existing Demo Guest"},actors.frontDesk)).id);
    const booking=await pms("reservation.create",{guestId,source:"staff",rooms:[{typeId:types[occupied.type],rateId:rates[occupied.type],arrival:date,departure:plusDays(date,2),adults:2,quantity:1}]},actors.frontDesk);
    const stay=records(await loadPms(db,admin,propertyId),"stay").find(item=>item.data.reservationId===booking.id);
    if(!stay)throw new Error("DEMO_OCCUPIED_STAY_MISSING");
    await pms("stay.assign",{id:stay.id,roomId:rooms[occupied.number]},actors.frontDesk);
    await pms("stay.check_in",{id:stay.id},actors.frontDesk);
  }
  const warehouseId=String((await saveMaster(admin,"warehouses",null,{branchId:f.branch,code:"RLH-KITCHEN",name:fixture.restaurant.warehouse},crypto.randomUUID())).id);
  const products:Record<string,string>={};
  for(const stock of fixture.restaurant.openingStock) {
    const menuItem=fixture.restaurant.menu.find(item=>item.stockSku===stock.sku);
    products[stock.sku]=String((await saveMaster(admin,"products",null,{sku:`RLH-${stock.sku}`,name:menuItem?.name??stock.sku,unit:stock.unit,price:String((menuItem?.priceMinor??100)/100),taxBps:menuItem?fixture.property.taxAndService.vatBpsExampleOnly:0},crypto.randomUUID())).id);
  }
  for(const item of fixture.restaurant.menu)products[item.sku]=item.stockSku?products[item.stockSku]:String((await saveMaster(admin,"products",null,{sku:`RLH-${item.sku}`,name:item.name,unit:"each",price:String(item.priceMinor/100),taxBps:fixture.property.taxAndService.vatBpsExampleOnly},crypto.randomUUID())).id);
  // Opening inventory is fixture-only evidence in a disposable database; it is not a production import.
  for(const stock of fixture.restaurant.openingStock)await input.db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,?,?,1)").bind(warehouseId,products[stock.sku],stock.quantityMilli,stock.valueMinor).run();
  const openingEquityId=`${f.company}-hospitality-opening-equity`;
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'HOTEL-OPEN','Demo opening inventory equity','equity','THB',?,?)").bind(openingEquityId,f.chart,f.company,now,now).run();
  const openingValue=fixture.restaurant.openingStock.reduce((sum,item)=>sum+item.valueMinor,0);
  const {createJournal,createJournalSchema,postJournal}=await import("../../src/server/finance");
  const {resolveFlow,assignFlowPrimary}=await import("../../src/server/flow-routing");
  const opening=await createJournal(admin,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:date,currency:"THB",memo:"Fictional Riverside kitchen opening inventory",sourceType:"manual",isOpening:true,lines:[{accountId:pmsAccounts.inventory,debit:String(openingValue/100),credit:"0"},{accountId:openingEquityId,debit:"0",credit:String(openingValue/100)}]}),crypto.randomUUID());
  const flow=await resolveFlow(db,admin,"journal",String(opening.id));
  await assignFlowPrimary(db,admin,"journal",String(opening.id),{stepKey:"draft",userId:finance.userId,expectedVersion:flow.version,recordVersion:1,reason:"Independent review of fictional opening kitchen inventory",saveDefault:false},crypto.randomUUID());
  await postJournal(finance,String(opening.id),crypto.randomUUID(),{expectedVersion:1});
  const pos=async(operation:string,details:unknown)=>posCommand(db,admin,operation,details,crypto.randomUUID());
  await pos("settings.save",{enabled:true,currency:"THB",timezone:"Asia/Bangkok",cashierDiscountBps:0,accounts:{chartId:f.chart,...posAccounts},expectedVersion:0});
  await posCommand(db,finance,"accounting.approve",{expectedVersion:1},crypto.randomUUID());
  const registerId=String((await pos("register.save",{warehouseId,propertyId,name:fixture.restaurant.register,mode:"restaurant"})).id);
  for(const table of fixture.restaurant.tables)await pos("table.create",{registerId,name:table,seats:4});
  for(const item of fixture.restaurant.menu)await pos("catalog.save",{productId:products[item.sku],barcode:"",category:"Restaurant",kind:item.recipe?"recipe":"stock",station:item.sku==="TEA"||item.sku==="COFFEE"?"Bar":"Kitchen",recipe:item.recipe?.map(part=>({productId:products[part.sku],quantityMilli:part.quantityMilli}))??[],modifiers:[],taxInclusive:false,expectedVersion:0});
  if(input.menuImages) {
    const {savePosProductImage}=await import("../../src/server/pos-images");
    const images:Record<string,string>={"RICE-CHICKEN":"menu-basil-chicken","RICE-VEG":"menu-fried-rice",NOODLES:"menu-noodles",TEA:"menu-thai-tea",COFFEE:"menu-latte",WATER:"menu-water"};
    for(const item of fixture.restaurant.menu) {
      const image=images[item.sku];
      if(!image)throw new Error("HOSPITALITY_MENU_IMAGE_REQUIRED");
      await savePosProductImage(admin,products[item.sku],new Uint8Array(await readFile(resolve(import.meta.dir,`../../fixtures/pos/images/${image}.png`))),"image/png","generated",crypto.randomUUID(),1,{fixture:image,generatedFor:"Fictional isolated hospitality catalog"});
    }
  }
  const guide={version:1,kind:"isolated-real-workflow",title:fixture.name,origin:input.origin,companyId:f.company,propertyId,registerId,reset:{destroy:`bun run demo:destroy -- ${input.run}`,prepare:"bun run demo:prepare"},sandbox:true,actors:users,
    scenarios:[{id:"restaurant-room-charge",startingState:`Room 101 is inspected on ${date}; room ${occupied?.number??"202"} has an existing checked-in example guest. No restaurant bill exists.`,steps:[
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/reservations?property=${propertyId}`,action:"Create a reservation, assign room 101, and check in the guest.",expected:"The stay appears as in-house with an open guest account and inspected room."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Open a restaurant bill and add Basil chicken rice and Thai iced tea.",expected:"The bill remains open with a reviewable THB total."},
      {actor:"kitchen",href:`${input.origin}/holdings/riverside-lantern/pos?tab=kitchen`,action:"Prepare and deliver the kitchen items.",expected:"Each recipe item reaches Delivered and stock is consumed once."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Charge the complete bill to the in-house guest account.",expected:"The bill is completed with one linked PMS charge and no POS payment."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/finance`,action:"Review the PMS posting and reconcile the linked source.",expected:"PMS owns the revenue posting; POS does not duplicate it."},
      {actor:"nightAuditor",href:`${input.origin}/holdings/riverside-lantern/pms/night-audit?property=${propertyId}`,action:"Review day-close blockers and close only after reconciliation.",expected:"An unresolved blocker prevents close; a resolved day has one recoverable audit result."}
    ],expected:"Exactly one POS sale, PMS guest account charge and PMS revenue posting remain linked after reload."},
    {id:"pending-bank-transfer",startingState:"The cashier creates a new unpaid restaurant bill in the reset run.",steps:[
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Record a bank-transfer reference without imported bank evidence.",expected:"The payment stays pending and the bill cannot complete."},
      {actor:"financePreparer",href:`${input.origin}/holdings/riverside-lantern/finance/bank`,action:"Import matching sandbox bank evidence.",expected:"The bank row is available for independent verification; no money moves outside the demo."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/finance/restaurants`,action:"Verify the exact amount, reference, currency, company and bank account.",expected:"One bank match settles one POS payment; an incorrect row remains unmatched."}
    ],expected:"Pending and verified amounts are shown separately."},
    {id:"hotel-verified-transfer",startingState:"The checked-in example guest has an open hotel guest account. No hotel bank transfer has been recorded.",steps:[
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/cashier?property=${propertyId}`,action:"Open a hotel cashier shift, then record a bank-transfer reference against the guest account.",expected:"The transfer is pending; the guest account and bank ledger remain unchanged."},
      {actor:"financePreparer",href:`${input.origin}/holdings/riverside-lantern/finance/bank`,action:"Import a fictional THB statement row for the hotel's configured bank account, date, exact amount and reference.",expected:"The row is available to an independent Finance approver without settling the guest account."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/finance/hotels`,action:"Select the exact imported row and verify the pending hotel transfer.",expected:"One PMS guest account receipt, Finance payment and matched bank row are linked and survive reload."}
    ],expected:"A typed reference never settles the guest account by itself; cancellation leaves no payment."},
    {id:"checkout-and-day-close",startingState:"The checked-in example guest has an open guest account and the hotel business day is open. Use this after any restaurant room charge you want included.",steps:[
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/front-desk?property=${propertyId}`,action:"Post the room night before checkout and review the guest account balance.",expected:"The room charge appears once on the same guest account."},
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/guest-accounts?property=${propertyId}`,action:"Open a hotel cash shift and collect the remaining guest account balance using cash or an independently verified transfer.",expected:"The guest account balance is zero and the receipt survives reload."},
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/front-desk?property=${propertyId}`,action:"Check out the guest after the balance is settled.",expected:"The stay is checked out and the departure room needs housekeeping."},
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/guest-accounts?property=${propertyId}`,action:"Issue the guest account invoice, count the hotel cash shift, and record the handover.",expected:"The invoice is paid, the guest account is closed, and the shift has no unexplained variance."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos?tab=shifts`,action:"Resolve or cancel any open restaurant bill, then count and close the restaurant cash session.",expected:"No open restaurant bill or cash session blocks the hotel day."},
      {actor:"nightAuditor",href:`${input.origin}/holdings/riverside-lantern/pms/night-audit?property=${propertyId}`,action:"Review the hotel and restaurant controls, then start the recoverable day-close job.",expected:"The worker completes the close, advances the business date, and keeps journals balanced."},
      {actor:"administrator",href:`${input.origin}/holdings/riverside-lantern/pms/night-audit?property=${propertyId}`,action:"If the initiating auditor loses access and the job blocks, review the reason and resume it with current authority.",expected:"The blocked date stays frozen until the authorized resume succeeds; no room night posts twice."}
    ],expected:"The guest account invoice is paid, restaurant cash is counted, and the next business date opens only after Night Audit completes."},
    {id:"wrong-room-recovery",startingState:"A checked-in example guest is in room 202 and a new restaurant bill is open.",steps:[
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Attempt to charge a nonexistent or checked-out stay.",expected:"The bill remains open with no guest account entry, stock consumption or posting."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Choose the verified in-house stay and retry with a fresh version.",expected:"Exactly one charge is created; repeating the same command key returns it."}
    ],expected:"Failed attempts do not duplicate revenue or stock movements."}]};
  return {companyId:f.company,propertyId,registerId,warehouseId,openingJournalId:opening.id,openingInventoryAccountId:pmsAccounts.inventory,roomIds:rooms,roomTypeIds:types,rateIds:rates,productIds:products,supplierIds:[supplierOne.id,supplierTwo.id],purchaseRequestId:purchaseRequest.id,payrollGroupId,payrollEngagementIds,payrollPersonIds,leaveTypeId,leavePolicyId,leaveAssignmentId,rosterWeekId,rosterShiftId,users,guide};
}
