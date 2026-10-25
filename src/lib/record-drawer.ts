/** Drawer references select an owner reader; they never confer authority. */
export const recordRoutes = {
  finance_group_edition:{label:"Group reporting edition",route:"record-details/finance_group_edition/:id"},
  finance_group_ownership:{label:"Ownership evidence",route:"record-details/finance_group_ownership/:id"},
  finance_group_acquisition:{label:"Acquisition evidence",route:"record-details/finance_group_acquisition/:id"},
  finance_group_pair:{label:"Paired Company documents",route:"record-details/finance_group_pair/:id"},
  finance_group_sources:{label:"Retained group account sources",route:"record-details/finance_group_sources/:id"},
  finance_group_source:{label:"Retained group journal",route:"record-details/finance_group_source/:id"},

  agent_pool:{label:"Agent pool",route:"record-details/agent_pool/:id"},
  agent_invocation:{label:"Agent invocation evidence",route:"record-details/agent_invocation/:id"},
  agent_session:{label:"Private session retention",route:"record-details/agent_session/:id"},
  finance_instrument:{label:"Lease or loan edition",route:"record-details/finance_instrument/:id"},
  finance_instrument_run:{label:"Instrument payment evidence",route:"record-details/finance_instrument_run/:id"},
  finance_asset_policy:{label:"Depreciation policy",route:"record-details/finance_asset_policy/:id"},
  finance_asset_policy_run:{label:"Depreciation evidence",route:"record-details/finance_asset_policy_run/:id"},
  finance_business_contract:{label:'finance business contract',route:'record-details/finance_business_contract/:id'},
  finance_business_bill:{label:'finance business bill',route:'record-details/finance_business_bill/:id'},
  finance_recognition:{label:'finance recognition',route:'record-details/finance_recognition/:id'},
  finance_obligation:{label:'finance obligation',route:'record-details/finance_obligation/:id'},
  bank_match_group:{label:'bank match group',route:'record-details/bank_match_group/:id'},
  bank_profile:{label:'bank profile',route:'record-details/bank_profile/:id'},
  collection_case:{label:'collection case',route:'record-details/collection_case/:id'},
  cash_forecast_sources:{label:'cash forecast sources',route:'record-details/cash_forecast_sources/:id'},

  pms_snapshot_entry:{label:'Document guest account entry',route:'record-details/pms_snapshot_entry/:id'},
  pms_booking_refund:{label:'Online booking refund',route:'record-details/pms_booking_refund/:id'},
  pms_counter_refund:{label:'Counter payment refund',route:'record-details/pms_counter_refund/:id'},
  pms_control_sources:{label:'PMS control account sources',route:'record-details/pms_control_sources/:id'},
  consolidation_elimination:{label:'Consolidation elimination',route:'record-details/consolidation_elimination/:id'},
  executive_sources: {label:'Report group sources',route:'record-details/executive_sources/:id'},
  executive_source: {label:'Report source record',route:'record-details/executive_source/:id'},
  order: { label: 'Order', route: 'commerce/orders/:id' },
  invoice: { label: 'Invoice', route: 'finance/invoices/:id' },
  journal: { label: 'Journal', route: 'finance/journals/:id' },
  payment: { label: 'Payment', route: 'finance/payments/:id' },
  refund: { label: 'Refund', route: 'finance/refunds/:id' },
  supplier_batch: { label: 'Supplier payment batch', route: 'finance/supplier-batches/:id' },
  finance_asset: { label: 'Fixed asset', route: 'finance/assets/:id' },
  tax_document: { label: 'Tax document', route: 'finance/thai-documents/:id' },
  vat_entry: { label: 'VAT entry', route: 'finance/tax/entries/:id' },
  vat_correction: { label: 'VAT correction', route: 'finance/tax/corrections/:id' },
  vat_package: { label: 'VAT package', route: 'finance/tax/packages/:id' },
  statement_package: { label: 'Financial statements', route: 'finance/statement-packages/:id' },
  landed_cost: { label: 'Landed cost', route: 'landed-costs/:id' },
  request: { label: 'Request', route: 'requests/:id' },
  expense: { label: 'Expense', route: 'expenses/:id' },
  leave: { label: 'Leave request', route: 'tasks/people_payroll/:id' },
  staff_task: { label: 'Task', route: 'staff-tasks/:id' },
  stock_count: { label: 'Stock count', route: 'stock-counts/:id' },
  stock_transfer: { label: 'Stock transfer', route: 'stock-transfers/:id' },
  stock_transit: { label: 'Stock transit', route: 'stock-transit/:id' },
  rfq: { label: 'Supplier quotation request', route: 'procurement-rfqs/:id' },
  supplier_bill: { label: 'Supplier bill matching', route: 'supplier-bills/:id' },
  customer: { label: 'Customer', route: 'customers/:id' },
  customer_identity: { label: 'Customer identity review', route: 'customer-identity/:id' },
  customer_duplicate: { label: 'Customer duplicate review', route: 'customer-duplicates/:id' },
  customer_access: { label: 'Customer access', route: 'customer-access/:id' },
  customer_followup: { label: 'Customer follow-up', route: 'customer-followups/:id' },
  crm_lead: { label: 'Lead', route: 'crm/:id' },
  marketing_candidate: { label: 'Marketing handoff', route: 'marketing/:id' },
  support_ticket: { label: 'Support ticket', route: 'support/:id' },
  support_incident: { label: 'Support incident', route: 'support/incidents/:id' },
  support_session: { label: 'Support session', route: 'support/sessions/:id' },
  field_service: { label: 'Field service', route: 'field-service/:id' },
  asset: { label: 'Asset', route: 'assets/:id' },
  maintenance: { label: 'Maintenance work', route: 'maintenance/:id' },
  maintenance_plan: { label: 'Preventive maintenance', route: 'maintenance/plans/:id' },
  quality_hold: { label: 'Quality hold', route: 'quality/:id' },
  manufacturing_order: { label: 'Manufacturing order', route: 'manufacturing/:id' },
  ecommerce_checkout: { label: 'Checkout', route: 'ecommerce/:id' },
  person: { label: 'Person', route: 'people/:id' },
  payroll_run: { label: 'Payroll run', route: 'people/payroll/runs/:id' },
  payslip: { label: 'Payslip', route: 'people/payslips/:id' },
  overtime: { label: 'Overtime', route: 'overtime/:id' },
  clearance: { label: 'Retirement clearance', route: 'people/clearance/:id' },
  learning_assignment: { label: 'Learning assignment', route: 'learning/:id' },
  benefit_enrollment: { label: 'Benefit enrollment', route: 'benefits/:id' },
  talent_application: { label: 'Application', route: 'talent/:id' },
  project: { label: 'Project', route: 'projects/:id' },
  routine_template: { label: 'Routine template', route: 'routine-templates/:id' },
  handbook: { label: 'Knowledge article', route: 'handbook/:id' },
  legal: { label: 'Legal record', route: 'legal?record=:id' },
  pms_document: { label: 'Property document', route: 'pms/documents/:id' },
  operation: { label: 'Operational exception', route: 'operations/:id' },
  product: { label: 'Product', route: "record-details/product/:id" },
  party: { label: 'Customer / supplier', route: "record-details/party/:id" },
  warehouse: { label: 'Warehouse', route: "record-details/warehouse/:id" },
  stock_position: { label: 'Stock position', route: "record-details/stock_position/:id" },
  inventory_lot: { label: 'Lot / serial', route: "record-details/inventory_lot/:id" },
  trial_balance: { label: 'Account entries', route: "record-details/trial_balance/:id" },
  aging_bucket: { label: 'Aging documents', route: "record-details/aging_bucket/:id" },
  payroll_month: { label: 'Posted payroll', route: "record-details/payroll_month/:id" },
  finance_report: { label: "Financial report sources", route: "record-details/finance_report/:id" },
  finance_account: { label: 'Chart account', route: "record-details/finance_account/:id" },
  accrual_schedule: { label: "Accrual schedule", route: "record-details/accrual_schedule/:id" },
  accrual_run: { label: "Accrual posting", route: "record-details/accrual_run/:id" },
  price_list: { label: "Price list", route: "record-details/price_list/:id" },
  reorder_policy: { label: "Reorder policy", route: "record-details/reorder_policy/:id" },
  intercompany: { label: "Intercompany transaction", route: "record-details/intercompany/:id" },
  consolidation: { label: "Consolidation run", route: "record-details/consolidation/:id" },
  brand_draft: { label: "Brand draft", route: "record-details/brand_draft/:id" },
  brand_version: { label: "Brand version", route: "record-details/brand_version/:id" },
  agent_version: { label: "Agent version", route: "record-details/agent_version/:id" },
  agent_assignment: { label: "Agent assignment", route: "record-details/agent_assignment/:id" },
  manufacturing_spec: { label: "BOM / formula", route: "record-details/manufacturing_spec/:id" },
  planning_scenario: { label: "Planning scenario", route: "record-details/planning_scenario/:id" },
  ecommerce_listing: { label: "Listing", route: "record-details/ecommerce_listing/:id" },
  ecommerce_cart: { label: "Cart", route: "record-details/ecommerce_cart/:id" },
  ecommerce_channel: { label: "Sales channel", route: "record-details/ecommerce_channel/:id" },
  learning_course: { label: "Course", route: "record-details/learning_course/:id" },
  benefit_plan: { label: "Benefit plan", route: "record-details/benefit_plan/:id" },
  talent_requisition: { label: "Requisition", route: "record-details/talent_requisition/:id" },
  talent_candidate: { label: "Candidate", route: "record-details/talent_candidate/:id" },
  bank_transaction: { label: 'Bank movement', route: 'record-details/bank_transaction/:id' },
  bank_statement: { label: 'Bank statement', route: 'record-details/bank_statement/:id' },
  finance_period: { label: 'Accounting period', route: 'finance/close?periodId=:id' },
  marketing_campaign_sources: { label: 'Campaign sources', route: 'record-details/marketing_campaign_sources/:id' },
  calculation_run: { label: 'Calculation run', route: "record-details/calculation_run/:id" },
  pos_report: { label: 'POS report receipts', route: 'record-details/pos_report/:id' },
  pos_sale: { label: 'Receipt', route: 'pos?tab=sale&sale=:id' },
  statement_sources: { label: 'Statement source accounts', route: 'record-details/statement_sources/:id' },
  work_flow: { label: 'Work record and Flow', route: 'work/routing/:id' },
  inventory_movement: { label: 'Inventory movement', route: 'record-details/inventory_movement/:id' },
  finance_plan: { label: 'Finance plan edition', route: 'record-details/finance_plan/:id' },
  finance_book: { label: 'Book rule', route: 'record-details/finance_book/:id' },
  finance_budget_sources: { label: 'Budget actuals sources', route: 'record-details/finance_budget_sources/:id' },
  finance_report_snapshot: { label: 'Retained report', route: 'record-details/finance_report_snapshot/:id' },
  finance_report_row: { label: 'Report row sources', route: 'record-details/finance_report_row/:id' },
} as const;
export type RecordKind = keyof typeof recordRoutes;
export type RecordTarget = { kind: RecordKind; id: string; args?: Record<string, string> };
export function isRecordKind(value: string): value is RecordKind { return Object.hasOwn(recordRoutes, value); }
export function recordTarget(href: string, base: string): RecordTarget | null {
  const url = new URL(href, "https://erpx.invalid");
  if (url.origin !== "https://erpx.invalid" || !url.pathname.startsWith(base + "/") || url.searchParams.get("print") === "1") return null;
  const destination = url.pathname.slice(base.length + 1);
  const staticRoutes = new Set(["people/organization","people/payroll","people/payslips","people/settings","people/roster","people/access","people/hire-handoffs","people/clearance","support/incidents","support/sessions","support/sla","people/move","maintenance/plans","requests/new","expenses/new","staff-tasks/new"]);
  if (staticRoutes.has(destination) || destination.endsWith("/new")) return null;
  const flow=destination.match(/^work\/(routing|department-approvals)\/([^/]+)\/([^/]+)$/);
  if(flow)return {kind:'work_flow',id:decodeURIComponent(flow[3]),args:{...Object.fromEntries(url.searchParams),kind:decodeURIComponent(flow[2]),view:flow[1]}};
  if (destination === "legal" && url.searchParams.get("record")) return { kind: "legal", id: url.searchParams.get("record")!, args: Object.fromEntries(url.searchParams) };
  if (destination === 'finance/close' && url.searchParams.get('periodId')) return {kind:'finance_period',id:url.searchParams.get('periodId')!};
  if (destination === 'pos' && url.searchParams.get('sale')) return {kind:'pos_sale',id:url.searchParams.get('sale')!};
  for (const [kind, definition] of Object.entries(recordRoutes)) {
    if (!definition.route) continue;
    const pattern = definition.route.split(":id");
    if (!destination.startsWith(pattern[0])) continue;
    const id = destination.slice(pattern[0].length);
    if (!id || id.includes("/")) continue;
    try { return { kind: kind as RecordKind, id: decodeURIComponent(id), args: Object.fromEntries(url.searchParams) }; } catch { return null; }
  }
  return null;
}
export function recordDrawerHref(current: string, target: RecordTarget) {
  const url = new URL(current, "https://erpx.invalid");
  url.searchParams.set("detailKind", target.kind);
  url.searchParams.set("detailId", target.id);
  url.searchParams.delete("detailPage");
  if (target.args && Object.keys(target.args).length) url.searchParams.set("detailArgs", JSON.stringify(target.args));
  else url.searchParams.delete("detailArgs");
  return url.pathname + url.search + url.hash;
}
export function withoutRecordDrawer(current: string) {
  const url = new URL(current, "https://erpx.invalid");
  for (const name of ["detailKind", "detailId", "detailArgs", "detailPage", "detailMode"]) url.searchParams.delete(name);
  return url.pathname + url.search + url.hash;
}
export function recordCanonicalHref(base: string, target: RecordTarget) {
  const route = target.kind==='work_flow'?`work/${target.args?.view==='department-approvals'?'department-approvals':'routing'}/${encodeURIComponent(target.args?.kind??'')}/:id`:recordRoutes[target.kind].route;
  const url = new URL(`${base}/${route.replace(":id", encodeURIComponent(target.id))}`, "https://erpx.invalid");
  for (const [key, value] of Object.entries(target.args ?? {})) if (!["detailKind", "detailId", "detailArgs", "detailPage", "detailMode", ...(target.kind === "legal" ? ["record"] : [])].includes(key)) url.searchParams.set(key, value);
  return url.pathname + url.search;
}
