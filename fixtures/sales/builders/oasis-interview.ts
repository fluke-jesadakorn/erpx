import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export const oasisKitchen = {
  title: "Phuket Villa – Kitchen A",
  customer: "Andaman Villa Demo Customer",
  surveyTitle: "Measure Kitchen A wall and countertop",
  surveyResult: "Demo site survey: wall 3.6 m, countertop 2.4 m. Allow three walnut base cabinet modules and verify the sink cutout before fabrication.",
  quoteNote: "Phuket Villa – Kitchen A. Fictional portfolio demonstration. All quantities and prices are illustrative. Site survey: wall 3.6 m, countertop 2.4 m, walnut fronts.",
  items: [
    { sku: "DEMO-KT-BASE", name: "Walnut base cabinet module", unit: "set", itemType: "stock", priceMinor: 1_850_000, quantity: "2" },
    { sku: "DEMO-KT-TOP", name: "Quartz countertop", unit: "m", itemType: "stock", priceMinor: 750_000, quantity: "2.4" },
    { sku: "DEMO-KT-FIT", name: "Kitchen installation", unit: "job", itemType: "service", priceMinor: 1_200_000, quantity: "1" },
  ],
} as const;

/** Seed the starting records only. The browser must perform the survey and sales transactions. */
export async function seedOasisInterview(db: SqlDatabase, input: {
  run: string;
  target?: "browser" | "oasis-demo-staging";
  companyId: string;
  branchId: string;
  salespersonId: string;
  managerId: string;
  finance: { chart: string; receivable: string; revenue: string; tax: string; preparerId: string; reviewerId: string };
  dueDate: string;
}) {
  const { run, companyId, branchId, salespersonId, managerId, dueDate, finance } = input;
  const binding = await db.prepare("SELECT current_database() AS name, current_user AS role").first<{ name: string; role: string }>();
  const staging = input.target === "oasis-demo-staging";
  if (!/^[a-f0-9]{12}$/.test(run) || (staging
    ? binding?.name !== "erpx_stage_oasis_demo_h001" || binding.role !== "erpx_stage_oasis_demo_h001_app"
    : binding?.name !== `erpx_browser_${run}_h001_db`)) {
    throw new Error("OASIS_INTERVIEW_REQUIRES_ISOLATED_DATABASE");
  }

  const id = (kind: string) => `oasis-demo-${kind}-${run}`;
  const now = Date.now(), customerId = id("customer"), warehouseId = id("warehouse");
  const accountId = id("account"), contactId = id("contact");
  const items = oasisKitchen.items.map(item => ({ ...item, productId: id(item.sku.toLowerCase()) }));
  const cases = [
    { title: oasisKitchen.title, leadId: id("lead") },
    { title: `${oasisKitchen.title} (rehearsal)`, leadId: id("rehearsal") },
  ];
  const contact = "Fictional buyer. Record contact through this demo only.";
  const brief = "We need a built-in kitchen for a Phuket villa, with walnut fronts and a quartz top. Budget: THB 90,000. Please include installation. The wall and countertop dimensions still need a site measurement.";

  await db.batch([
    db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer','DEMO-KITCHEN',?,'THB',?,?)")
      .bind(customerId, companyId, oasisKitchen.customer, now, now),
    db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,'DEMO-KITCHEN','Andaman Demo Workshop')")
      .bind(warehouseId, companyId, branchId),
    ...items.map(item => db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,item_type,price_minor,tax_bps) VALUES(?,?,?,?,?,?,?,0)")
      .bind(item.productId, companyId, item.sku, item.name, item.unit, item.itemType, item.priceMinor)),
    db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='receivable' WHERE id=? AND company_id=?")
      .bind(finance.receivable, companyId),
    db.prepare("INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,status,reason,created_by,reviewed_by,review_reason,version,created_at,updated_at) VALUES(?,?,?,'THB',?,?,?,'approved',?,?,?, ?,2,?,?)")
      .bind(id("sales-defaults"), companyId, finance.chart, finance.receivable, finance.revenue, finance.tax,
        "Synthetic portfolio fixture", finance.preparerId, finance.reviewerId, "Independent fixture reviewer", now, now),
    ...[["submitted", managerId], ["confirmed", salespersonId]].map(([step, ownerId]) =>
      db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'order',?,?,?,?,?)")
        .bind(companyId, step, ownerId, finance.reviewerId, "Fictional interview routing", now)),
    db.prepare("INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,review_evidence,reviewed_by,created_by,created_at,updated_at) VALUES(?,?,'individual',?,?,?,?,?,?,?)")
      .bind(accountId, companyId, oasisKitchen.customer, customerId, "Synthetic customer identity linkage", managerId, salespersonId, now, now),
    db.prepare("INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
      .bind(contactId, companyId, accountId, oasisKitchen.customer, contact, salespersonId, now, now),
    ...cases.map(({ title, leadId }) => db.prepare(
      "INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at,account_id,contact_id) VALUES(?,?,?,?,?,?,'Showroom consultation (demo)',?,?,?,?,?,?,?,?)",
    ).bind(leadId, companyId, branchId, title, oasisKitchen.customer, contact, brief, salespersonId, dueDate, salespersonId, now, now, accountId, contactId)),
  ]);

  return { cases, customerId, warehouseId, items, dueDate };
}
