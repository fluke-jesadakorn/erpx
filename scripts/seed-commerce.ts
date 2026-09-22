import { getLocalProxy } from "./local/fixtures";
import { moduleRegistry } from "../src/data/module-registry";
import { businessRoles,type BusinessRole } from "../src/lib/business-roles";
import { hashPassword } from "better-auth/crypto";
const p=await getLocalProxy({configPath:"wrangler.jsonc",persist:true,remoteBindings:false});
const now=Date.now();
const personas=[
  ["henry","Henry Lawson","sales_operator","hr-acme-co-005"],
  ["sofia","Sofia Ramirez","sales_manager","hr-acme-co-008"],
  ["leila","Leila Haddad","purchasing_operator","hr-acme-co-006"],
  ["owen","Owen Brooks","warehouse_manager","hr-acme-co-007"],
  ["daniel","Daniel Kim","finance_preparer","hr-acme-co-009"],
  ["priya","Priya Shah","purchasing_manager","hr-acme-co-010"],
] as const;
try {
  if(p.env.ERPX_ENVIRONMENT!=="local")throw new Error("This scenario seed is local only.");
  for(const [first,name,role,personId] of personas) {
    const user=`usr_${first}`;const hash=await hashPassword(`${name.split(" ")[0]}-ERPX-2026!`);
    await p.env.CONTROL_DB.batch([
      p.env.CONTROL_DB.prepare("INSERT INTO \"user\"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,email=excluded.email,emailVerified=1,updatedAt=excluded.updatedAt").bind(user,name,`${first}@erpx.dev`,now,now),
      p.env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?) ON CONFLICT(id) DO UPDATE SET password=excluded.password,updatedAt=excluded.updatedAt").bind(`acc_${user}`,user,user,hash,now,now),
      p.env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,status='active'").bind(`pri_${user}`,user,name,now),
    ]);
    await p.env.H001_DB.batch([
      p.env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,?,?,'acme-co',NULL,?,?,'holding',NULL) ON CONFLICT DO NOTHING").bind(`workflow_${first}`,user,role,JSON.stringify(businessRoles[role as BusinessRole]),now-1000,now+365*86400000),
      p.env.H001_DB.prepare("UPDATE hr_people SET user_id=?,email=? WHERE id=? AND (user_id IS NULL OR user_id=?)").bind(user,`${first}@erpx.dev`,personId,user),
      p.env.H001_DB.prepare("UPDATE employees SET user_id=? WHERE person_id=? AND (user_id IS NULL OR user_id=?)").bind(user,personId,user),
    ]);
  }
  await p.env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('asg_mali_owner','usr_mali','holding_owner',?,'acme-co',?,?,'holding') ON CONFLICT DO NOTHING").bind(JSON.stringify(moduleRegistry.map(module=>module.id)),now-1000,now+365*86400000).run();
  // Controllers approve supplier payment; access is distinct from preparation.
  await p.env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('workflow_controller','usr_nisa','finance_approver','[\"finance\",\"people_payroll\"]','acme-co',?,?,'holding') ON CONFLICT DO NOTHING").bind(now-1000,now+365*86400000).run();
  // The holding owner also has an explicitly scoped treasury assignment for the
  // demo payment handoff, keeping the payer independent from the preparer.
  await p.env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('workflow_treasury','usr_mali','treasury','[\"finance\"]','acme-co',?,?,'holding') ON CONFLICT(id) DO UPDATE SET user_id=excluded.user_id,role=excluded.role,modules_json=excluded.modules_json,company_id=excluded.company_id,valid_from=excluded.valid_from,valid_until=excluded.valid_until,revoked_at=NULL").bind(now-1000,now+365*86400000).run();
  for(const [db,company,branch,currency] of [[p.env.H001_DB,"acme-co","bangkok","USD"],[p.env.H001_DB,"beacon-co","london","GBP"],[p.env.H002_DB,"northstar-co","rotterdam","EUR"]] as const) {
    const chart=`commerce-chart-${company}`;
    const accounts=[["1100","Operating bank","asset",1,"bank"],["1200","Trade receivables","asset",1,"receivable"],["1300","Inventory","asset",1,"none"],["1400","Recoverable input tax","asset",1,"none"],["2100","Trade payables","liability",1,"payable"],["2150","Goods received not invoiced","liability",1,"none"],["2200","Output tax payable","liability",1,"none"],["4000","Product revenue","revenue",0,"none"],["5000","Cost of goods sold","expense",0,"none"],["5100","Travel and transport","expense",0,"none"],["5200","Office supplies","expense",0,"none"],["5300","Professional development","expense",0,"none"],["5400","Stock count variance","expense",0,"none"]] as const;
    await db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(chart,company,"Operations ledger",currency,"system",now,now).run();
    for(const [code,name,category,isControl,controlKind]of accounts)await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET is_control=excluded.is_control,control_kind=excluded.control_kind,updated_at=excluded.updated_at").bind(`${chart}-${code}`,chart,company,code,name,category,currency,isControl,controlKind,now,now).run();
    await db.prepare("INSERT INTO commerce_settings VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(company,chart,currency,...["1200","2100","4000","1300","2150","5000","1400","2200","1100"].map(code=>`${chart}-${code}`)).run();
    await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`warehouse-${company}`,company,branch,"MAIN",`${branch.charAt(0).toUpperCase()+branch.slice(1)} distribution warehouse`).run();
    await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`warehouse-${company}-staging`,company,branch,"STAGING",`${branch.charAt(0).toUpperCase()+branch.slice(1)} installation staging warehouse`).run();
    for(const [sku,name,price]of [["DESK-120","Adjustable office desk 120 cm",35000],["CHAIR-ERG","Ergonomic office chair",18000],["MON-27","27-inch business monitor",26000]] as const)await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tax_bps) VALUES(?,?,?,?,?,0) ON CONFLICT DO NOTHING").bind(`product-${company}-${sku}`,company,sku,name,price).run();
    for(const [kind,code,name]of [["customer","CUS-OFFICE","Harbor Office Solutions"],["supplier","SUP-FURN","Summit Furniture Manufacturing"]] as const)await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,created_at,updated_at) VALUES(?,?,?,?,?,?,30,?,?) ON CONFLICT DO NOTHING").bind(`commerce-${company}-${kind}`,company,kind,code,name,currency,now,now).run();
    await db.prepare("UPDATE finance_parties SET notification_email=?,notification_consent='granted' WHERE id=? AND notification_email IS NULL").bind(`receiving-${company}@example.invalid`,`commerce-${company}-customer`).run();
    await db.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,updated_by,updated_at) VALUES(?,'sandbox','local-demo-seed',?) ON CONFLICT DO NOTHING").bind(company,now).run();
    console.log(`Configured ${company}: ${currency} ledger, 3 products, customer, supplier and two warehouses`);
  }
}finally{await p.dispose();}
