/* Active feature coverage for the browser-local enterprise prototype. */
const CAPABILITY_MANIFEST = {
  brand: [["Draft workspace","Create, save and preview versioned identity drafts"],["Category locks","Protect approved identity categories"],["Published identity","Publish, inspect history and roll back a version"]],
  commercial: [["Catalog plans","Create and publish plans"],["Subscriptions","Inspect checkout, portal and provider-mirrored status"],["Invoices","Review provider-mirrored invoices and billing events"],["Entitlements","Allocate, activate and change company or branch access"],["Grace & recovery","Grant grace and recover failed billing events"]],
  identity_access: [["People & principals","Create, activate, suspend and recover principals"],["Roles & permissions","Define roles and review permission sets"],["Assignments","Assign, revoke and renew expiring roles"],["Directory connectors","Preflight, import, map and reconcile directories"],["Identity conflicts","Approve or reject replacement proposals"],["Access reviews","Inspect expiring roles and access evidence"]],
  organization: [["Organization tree","Create, rename and place organization nodes"],["Node lifecycle","Activate, suspend and archive nodes"],["Business units","Inspect companies, branches and their configuration"],["Sharing","Review explicit audience and branch agreements"]],
  platform_administration: [["Holdings","Register, provision, activate and suspend holdings"],["Provisioning","Provision branches and retry failed work"],["Connectors","Register and transition platform connectors"],["Infrastructure","Review desired configuration and enrollment"],["Backup & recovery","Schedule backups and run restore rehearsals"],["Projections & events","Rebuild projections and replay failed events"],["Migration runs","Queue work and advance migration phases"],["Temporary access","Request, approve and revoke audited access"],["Platform audit","Inspect operational and control history"]],
  customer_360: [["Customer profile","Read the composed customer and owning sources"],["Sales summary","Open related opportunities, quotes and orders"],["Service timeline","Open customer conversations and support cases"],["Customer notes","Add a scoped note with source references"]],
  customer_authority: [["Authority bindings","Create, link and unlink customer authority"],["Identity changes","Update identity and binding status"],["Duplicate review","Compare and merge customer profiles"],["Access history","Inspect the customer access audit"]],
  support: [["Tickets","Open, triage, reply and resolve customer tickets"],["SLA watch","Prioritize tickets nearing their response target"],["Incidents","Open incidents, add timeline events and broadcast status"],["Support sessions","Request, approve and revoke customer sessions"],["Capabilities","Issue and revoke scoped support capabilities"],["Attachments","Share customer-safe evidence or private internal files"]],
  finance: [["Chart of accounts","Import a chart, add accounts and change status"],["Journals","Create, balance, post and inspect multi-currency journals"],["Payables","Create, post and match supplier invoices"],["Payments","Create, approve and release payments"],["Receivables & aging","Inspect customer balances and aging"],["Bank reconciliation","Import CSV, review matches and reconcile"],["Reports","Review trial balance and financial statements"],["Period close","Begin, close and reopen periods with separation"],["Opening balances","Import and verify the opening trial balance"]],
  pos: [["Register session","Open and manage the branch register"],["Sales & baskets","Scan lines, hold, resume and complete sales"],["Payments & receipts","Record tender and inspect receipts"],["Voids","Review and void eligible sales"],["Refunds","Request, decide, post or cancel bounded refunds"]],
  sales: [["Quotes","Create, send, accept, reject and expire quotes"],["Orders","Create and confirm orders from accepted quotes"],["Fulfillment","Record partial or complete fulfillment"],["Cancellations","Cancel eligible orders with history"],["Revenue handoff","Open the connected CRM and finance work"]],
  assets: [["Asset register","Create and inspect assets"],["Placement","Place assets into service"],["Condition","Mark degradation and review condition history"],["Maintenance lifecycle","Begin maintenance and return to service"],["Retirement","Retire eligible assets with evidence"]],
  inventory: [["Stock register","Register items and opening availability"],["Receipts","Receive and put away stock"],["Issues & movements","Issue or move available stock"],["Reservations","Reserve and release quantities"],["Availability","Inspect the rebuildable stock projection"]],
  maintenance: [["Work orders","Open, schedule, start and cancel orders"],["Blocked work","Block and resume work with a reason"],["Downtime","Record operational downtime"],["Completion","Complete with required evidence"],["Maintenance history","Inspect asset-linked history"]],
  operations: [["Exception queue","Acknowledge, assign and prioritize exceptions"],["Escalations","Escalate exceptions to an operations lead"],["Approval decisions","Record reviewed operational decisions"],["Resolution","Resolve and close exceptions"],["Queue policies","Publish and inspect work-queue policies"],["Operations summary","Inspect area health and handovers"]],
  procurement: [["Requisitions","Create, submit, approve, reject or cancel requests"],["Purchase orders","Create, release and cancel supplier orders"],["Goods receipts","Create, accept, reject or cancel receipts"],["Invoice match","Open related receipt and finance evidence"],["Supplier history","Inspect connected supplier work"]],
  quality: [["Inspections","Create and start quality checks"],["Results","Record measurements and evidence"],["Release gates","Review and release passing work"],["Cancellation","Cancel eligible checks without losing history"]],
  legal: [["Contracts","Create contracts, add parties and transition status"],["Documents","Create legal documents and add versions"],["Legal holds","Show holds and block incompatible deletion"],["Document access","Record and inspect document access"],["Archive","Archive eligible documents with history"]],
  people_payroll: [["Employees","Hire and transition employee status"],["Leave requests","Create and decide leave requests"],["Expenses","Create and decide expense claims"],["Payroll runs","Create, review and transition payroll runs"],["Payroll summaries","Show restricted payroll projections"]],
  crm: [["Accounts","Inspect customer accounts and shared conversations"],["Leads","Accept marketing leads and inspect attribution"],["Opportunities","Create, save and transition opportunities"],["Activities","Record the next customer interaction"],["Sales handoff","Prepare and open the related sales quote"]],
  marketing: [["Forms & profiles","Capture forms and honor profile erasure"],["Content & previews","Issue and revoke scoped previews"],["Domains","Verify and bind marketing domains"],["Provider readiness","Configure and acknowledge provider accounts"],["Lead candidates","Create, qualify or reject candidates"],["Engagement","Capture attribution and customer activity"]],
  manufacturing: [["Specifications","Publish product specifications"],["Orders","Create, release, start and close manufacturing orders"],["Material readiness","Reserve and issue material"],["Labor & output","Report labor, result and yield"],["Quality disposition","Apply reviewed quality decisions"],["Costs & genealogy","Record cost and inspect genealogy or WIP"]],
  planning: [["Scenarios","Create, freeze and run planning scenarios"],["Shortages","Inspect material shortages"],["Capacity","Inspect capacity overloads"],["Recommendations","Override, approve, reject and release recommendations"],["Plan comparison","Compare released plan outcomes"]],
  projects: [["Projects & WBS","Create projects; add, move and transition nodes"],["Dependencies","Propose, decide, revoke and cancel dependencies"],["Handoffs","Propose, approve, mark ready and reconcile handoffs"],["Time & costs","Record node time and cost"],["Attachments","Attach files and related records to WBS nodes"],["Project graph","Inspect the connected project graph"]],
  ecommerce: [["Channels","Create, approve and transition channels"],["Listings","Create, approve, publish or withdraw listings"],["Carts","Build and lock customer carts"],["Checkout","Reconcile validation, stock, payment and order stages"],["Fulfillment","Propose and transition fulfillment"],["Returns","Request returns and reconcile disposition or refund"]],
  benefits: [["Plans","Publish benefit plans"],["Enrollment windows","Open and inspect enrollment periods"],["Dependents","Add and validate dependents"],["Coverage","Elect and validate coverage"],["Provider & payroll","Submit, acknowledge and release payroll changes"]],
  learning: [["Course catalog","Publish and retire courses"],["Assignments","Assign courses to permitted people"],["Attendance & results","Record attendance and provider results"],["Completion","Complete or waive enrollments"],["Eligibility handoff","Open related benefits eligibility"]],
  talent: [["Requisitions","Create, approve and transition requisitions"],["Candidates & privacy","Create profiles, holds and erasure requests"],["Applications","Submit, interview, select or withdraw applications"],["Offers","Create, approve, present and decide offers"],["Hire handoffs","Request and reconcile employee onboarding"],["Proposals","Create, submit, approve and reconcile proposals"]],
  field_service: [["Agreements","Activate service agreements with approval"],["Service orders","Create and schedule orders"],["Dispatch","Reserve parts and dispatch technicians"],["Visits","Start visits, issue parts and capture evidence"],["Exceptions","Record site or service exceptions"],["Billing release","Complete visits and release connected billing"]],
  agent: [["Agent catalog","Register, suspend and retire agent definitions"],["Versions","Add and approve agent versions"],["Pools & routing","Register pools, replace members and inspect routing"],["Assignments","Start, delegate, retry or cancel assignments"],["Private sessions","Run private, held or expiring sessions"],["Field suggestions","Request, review, apply or reject evidence-backed changes"],["Invocation evidence","Inspect attributable agent activity"]],
  collaboration: [["Threads","Create, search and resolve record conversations"],["Comments & moderation","Reply, edit and tombstone comments"],["Mentions","Autocomplete scoped people and record targets"],["Following & reactions","Follow work and add non-decision signals"],["Topics & feed","Publish posts and follow governed hashtags"],["Attachments","Upload and download authorized thread files"],["Action cards","Propose and confirm reviewed actions"],["Record relations","Add, order, inspect and revoke typed links"]],
};

function capabilitySlug(label) { return "feature-" + slug(label); }
function capabilityTabs(k) {
  return (CAPABILITY_MANIFEST[k] || []).map(([label]) => ({
    label,
    id: (MODULES[k]?.tabs || []).some((tab) => slug(tab) === slug(label))
      ? slug(label)
      : capabilitySlug(label),
  }));
}
function capabilityFor(k, section) { return capabilityTabs(k).find((x) => x.id === section); }
function capabilityKind(k, label) {
  const value = `${k} ${label}`.toLowerCase();
  if (/availability|reports|summary|aging|comparison|costs|genealogy|readiness|capacity|shortages/.test(value)) return "analytics";
  if (/journal|payable|payment|receivable|bank reconciliation|period close|opening balance|invoice/.test(value)) return "ledger";
  if (/schedule|timeline|history|activity|access|audit|downtime/.test(value)) return "timeline";
  if (/pipeline|application|wbs|dependenc|handoff|queue|fulfillment|checkout|return/.test(value)) return "board";
  if (/document|attachment|content|preview|listing|catalog|course|plan/.test(value)) return "library";
  if (/connector|infrastructure|backup|projection|migration|domain|provider|provision|health/.test(value)) return "health";
  if (/account|profile|people|principal|employee|candidate|dependent|customer/.test(value)) return "directory";
  if (/thread|comment|mention|conversation|private session/.test(value)) return "conversation";
  return "workflow";
}
function capabilityActionLabel(k, label) {
  if (moduleReadOnly(k)) return "Refresh view";
  const lower = label.toLowerCase();
  if (/availability|reports|summary|history|audit|aging|comparison/.test(lower)) return `Review ${lower}`;
  if (/import|opening balance/.test(lower)) return "Import file";
  if (/attachment|document|content/.test(lower)) return "Add file";
  if (/schedule/.test(lower)) return "Schedule work";
  if (/account|people|employee|candidate|dependent/.test(lower)) return `Add ${lower.replace(/s$/, "")}`;
  if (/reconciliation|close|approval|review/.test(lower)) return `Start ${lower}`;
  return `Update ${lower}`;
}
function capabilityMetrics(k, rows) {
  const completed = rows.filter((x) => x.finished || /active|posted|closed|complete|published|received|released/i.test(x.status)).length;
  const attention = rows.filter((x) => /draft|open|expected|review|blocked|exception|pending/i.test(x.status)).length;
  const labels = k === "inventory"
    ? [["Items in scope", rows.length], ["Units available", rows.reduce((n, x) => n + Number(x.fields?.quantity || 0), 80)], ["Reservations", Math.max(1, attention)], ["Stock health", attention ? "Watch" : "Healthy"]]
    : [["Records in scope", rows.length], ["Needs attention", attention], ["Completed", completed], ["Data status", freshness()]];
  return `<div class="feature-metrics">${labels.map(([label, value]) => `<div><small>${esc(label)}</small><strong>${esc(value)}</strong></div>`).join("")}</div>`;
}
function capabilityTable(k, rows) {
  const candidates = rows.slice(0, 6);
  return `<div class="table-wrap feature-table" tabindex="0" role="region" aria-label="${esc(SEED.CTX[k].n)} records"><table><thead><tr><th>Record</th><th>Type</th><th>Owner / location</th><th>Status</th><th></th></tr></thead><tbody>${candidates.map((x) => `<tr><td><a href="${recordRoute(x)}"><strong>${esc(x.title)}</strong><small>${esc(x.id)}</small></a></td><td>${esc(x.recordType?.replaceAll("_", " ") || x.subtitle)}</td><td>${esc(x.fields?.owner || x.fields?.assigned || branchName(x.branch))}</td><td>${badge(x.status)}</td><td><a class="icon-btn" href="${recordRoute(x)}" aria-label="Open ${esc(x.title)}">${icon("chevron")}</a></td></tr>`).join("")}</tbody></table></div>`;
}
function inventoryAvailability(rows) {
  return `<section class="card card-pad feature-primary"><div class="between wrap"><div><h2>Availability by item</h2><p class="small muted">On-hand stock minus reservations, within the selected branches.</p></div><div class="segmented"><button class="active">All items</button><button>Low stock</button></div></div><div class="table-wrap"><table><thead><tr><th>Item</th><th>Location</th><th>On hand</th><th>Reserved</th><th>Available</th><th>Status</th></tr></thead><tbody>${rows.slice(0, 6).map((x, i) => { const onHand = 80 + Number(x.fields?.quantity || i * 12), reserved = Math.min(onHand, 12 + i * 7), available = onHand - reserved; return `<tr><td><a href="${recordRoute(x)}"><strong>${esc(x.fields?.sku || x.title)}</strong><small>${esc(x.id)}</small></a></td><td>${esc(x.fields?.location || branchName(x.branch))}</td><td>${onHand}</td><td>${reserved}</td><td><strong>${available}</strong></td><td>${badge(available < 30 ? "Needs review" : "Available")}</td></tr>`; }).join("")}</tbody></table></div></section><section class="feature-side card card-pad"><h2>Stock signals</h2><div class="signal-list"><div><span class="signal-dot amber"></span><span><strong>2 reservations due today</strong><small>Sales and project demand</small></span></div><div><span class="signal-dot green"></span><span><strong>Receiving is on schedule</strong><small>Next delivery · 10:30</small></span></div><div><span class="signal-dot"></span><span><strong>1 replenishment proposed</strong><small>Waiting for planner review</small></span></div></div><a class="btn" href="${route("journeys/supply")}">Open supply journey ${icon("arrow")}</a></section>`;
}
function capabilityBoard(k, rows, r) {
  const stages = (flowSteps(r).length ? flowSteps(r).map((x) => x.status) : ["Planned", "In progress", "Review", "Complete"]).slice(0, 4);
  return `<section class="feature-board">${stages.map((stage, i) => `<div><header><h3>${esc(stage)}</h3><span>${rows.filter((x) => x.status === stage).length || (i === 0 ? rows.length : 0)}</span></header>${(rows.filter((x) => x.status === stage).length ? rows.filter((x) => x.status === stage) : i === 0 ? rows : []).slice(0, 3).map((x) => `<a class="board-record" href="${recordRoute(x)}"><strong>${esc(x.title)}</strong><small>${esc(x.id)} · ${esc(branchName(x.branch))}</small>${badge(x.status)}</a>`).join("") || '<p class="tiny muted">No work in this stage.</p>'}</div>`).join("")}</section>`;
}
function capabilityTimeline(k, rows, r) {
  const events = rows.flatMap((x) => x.history.slice(-2).map((h) => ({ ...h, record: x }))).slice(0, 7);
  return `<section class="card card-pad feature-primary"><div class="between"><div><h2>Recent activity</h2><p class="small muted">A complete, attributable history for this workspace.</p></div><button class="btn">Export history</button></div><div class="feature-timeline">${events.map((e) => `<a href="${recordRoute(e.record)}"><span></span><div><strong>${esc(e.text)}</strong><p>${esc(e.record.title)}</p><small>${esc(e.by)} · ${esc(e.time)}</small></div></a>`).join("")}</div></section><section class="feature-side card card-pad"><h2>Review context</h2>${recordLinkCard(r)}<div class="summary-facts vertical"><div><small>Retention</small><strong>Audit history kept</strong></div><div><small>Visibility</small><strong>${esc(scopeLabel())}</strong></div></div></section>`;
}
function capabilityAnalytics(k, rows, r, label) {
  if (k === "inventory" && label === "Availability") return inventoryAvailability(rows);
  const values = [64, 82, 48, 91, 73, 58];
  return `<section class="card card-pad feature-primary"><div class="between wrap"><div><h2>${esc(label)} overview</h2><p class="small muted">Current scoped projection with source records attached.</p></div><select aria-label="Reporting period"><option>This month</option><option>Previous month</option></select></div><div class="feature-chart" aria-label="Six-period trend">${values.map((v, i) => `<div><span style="height:${v}%"></span><small>${["Apr","May","Jun","Jul","Aug","Sep"][i]}</small></div>`).join("")}</div>${capabilityTable(k, rows)}</section><section class="feature-side card card-pad"><h2>Highlights</h2><div class="insight"><strong>${rows.length}</strong><span>visible records</span></div><div class="insight"><strong>${rows.filter((x) => !x.finished).length}</strong><span>still moving</span></div>${recordLinkCard(r)}</section>`;
}
function capabilityLedger(k, rows, r, label) {
  return `<section class="card card-pad feature-primary"><div class="between wrap"><div><h2>${esc(label)}</h2><p class="small muted">Balanced, reviewable entries for the selected company and period.</p></div><span class="badge green">${icon("check")}Controls active</span></div>${capabilityTable(k, rows)}<div class="ledger-total"><span>Visible balance</span><strong>${money(rows.reduce((n, x) => n + Number(x.fields?.debit || x.fields?.amount || 1200), 0))}</strong></div></section><section class="feature-side card card-pad"><h2>Period controls</h2><div class="summary-facts vertical"><div><small>Company</small><strong>${esc(primaryCompany(r))}</strong></div><div><small>Period</small><strong>September · Open</strong></div><div><small>Approval</small><strong>Different reviewer required</strong></div></div><a class="btn" href="${recordRoute(r)}">Review source entry ${icon("arrow")}</a></section>`;
}
function capabilityDirectory(k, rows, r, label) {
  return `<section class="card card-pad feature-primary"><div class="between wrap"><div><h2>${esc(label)}</h2><p class="small muted">Profiles and relationships available in this work context.</p></div><form class="inline-search"><input aria-label="Search directory" placeholder="Search name, ID or owner…"><button class="btn">Filter</button></form></div><div class="directory-grid">${rows.slice(0, 6).map((x) => `<a href="${recordRoute(x)}"><span class="directory-avatar">${esc((x.title.match(/\b[A-Z]/g) || ["E"]).slice(0, 2).join(""))}</span><span class="grow"><strong>${esc(x.title)}</strong><small>${esc(x.subtitle)} · ${esc(x.id)}</small></span>${badge(x.status)}${icon("chevron")}</a>`).join("")}</div></section><section class="feature-side card card-pad"><h2>Account context</h2>${recordLinkCard(r)}<a class="btn" href="${recordRoute(r)}?view=discussion">Open shared conversation ${icon("message")}</a><p class="tiny muted">The conversation stays attached to the owning record across CRM, Sales, Support and Finance links.</p></section>`;
}
function capabilityLibrary(k, rows, r, label) {
  return `<section class="feature-primary"><div class="document-grid">${rows.slice(0, 6).map((x, i) => `<a class="card document-card" href="${recordRoute(x)}"><div class="document-preview tone-${i % 3}">${icon(/attachment|document/i.test(label) ? "file" : "image")}</div><span class="badge gray">${esc(x.status)}</span><h3>${esc(x.title)}</h3><p>${esc(x.subtitle)} · ${esc(x.id)}</p><small>Updated ${i + 1}h ago</small></a>`).join("")}</div></section><section class="feature-side card card-pad"><h2>Selected item</h2>${recordLinkCard(r)}<div class="flex wrap"><a class="btn" href="${recordRoute(r)}">Open details</a><a class="btn" href="${recordRoute(r)}?view=discussion">Discuss</a></div></section>`;
}
function capabilityHealth(k, rows, r, label) {
  const checks = ["Configuration validated", "Scope and permissions checked", "Latest projection available", "Recovery path ready"];
  return `<section class="card card-pad feature-primary"><div class="between"><div><h2>${esc(label)} status</h2><p class="small muted">Operational readiness and the next safe action.</p></div><span class="health-score">96<small>% healthy</small></span></div><div class="health-checks">${checks.map((x, i) => `<div>${icon(i === 2 && state.demoState !== "normal" ? "alert" : "check")}<span><strong>${x}</strong><small>${i === 1 ? scopeLabel() : "Checked moments ago"}</small></span><span class="badge ${i === 2 && state.demoState !== "normal" ? "amber" : "green"}">${i === 2 && state.demoState !== "normal" ? "Review" : "Ready"}</span></div>`).join("")}</div></section><section class="feature-side card card-pad"><h2>Current operation</h2>${recordLinkCard(r)}<div class="notice">No customer business content is exposed in platform operations.</div></section>`;
}
function capabilityWorkflow(k, rows, r, label) {
  return `<section class="card card-pad feature-primary"><div class="between wrap"><div><h2>${esc(label)} workspace</h2><p class="small muted">Prioritized records with ownership, stage and the next required action.</p></div><div class="segmented"><button class="active">Active</button><button>Waiting</button><button>Completed</button></div></div>${capabilityTable(k, rows)}</section><section class="feature-side card card-pad"><h2>Next action</h2><p class="small muted">Continue the highest-priority record without losing its linked context.</p>${recordLinkCard(r)}<a class="btn primary" href="${taskRoute(r)}">${esc(taskAction(r)?.action || "Open record")} ${icon("arrow")}</a></section>`;
}
function capabilityConversation(k, rows, r) {
  return `<section class="feature-primary">${discussionPanel(r)}</section><section class="feature-side card card-pad"><h2>Conversation context</h2>${recordLinkCard(r)}<div class="summary-facts vertical"><div><small>Participants</small><strong>${new Set((r.comments || []).map((x) => x.by)).size || 1}</strong></div><div><small>Attachments</small><strong>${(r.comments || []).flatMap((x) => x.attachments || []).length}</strong></div><div><small>Visibility</small><strong>${esc(scopeLabel())}</strong></div></div><a class="btn" href="${route("inbox")}">Open mentions ${icon("arrow")}</a></section>`;
}
function capabilityBody(kind, k, rows, r, label) {
  if (kind === "analytics") return capabilityAnalytics(k, rows, r, label);
  if (kind === "ledger") return capabilityLedger(k, rows, r, label);
  if (kind === "timeline") return capabilityTimeline(k, rows, r);
  if (kind === "board") return capabilityBoard(k, rows, r);
  if (kind === "library") return capabilityLibrary(k, rows, r, label);
  if (kind === "health") return capabilityHealth(k, rows, r, label);
  if (kind === "directory") return capabilityDirectory(k, rows, r, label);
  if (kind === "conversation") return capabilityConversation(k, rows, r);
  return capabilityWorkflow(k, rows, r, label);
}
function capabilityView(k, section, rows) {
  const feature = capabilityFor(k, section), r = workflowRecord(k);
  if (!feature || !r) return "";
  const source = CAPABILITY_MANIFEST[k].find(([label]) => label === feature.label);
  const runKey = `${state.holding}:${k}:${section}`, run = state.capabilityRuns?.[runKey], kind = capabilityKind(k, feature.label);
  return `<div class="stack feature-page"><section class="feature-page-head"><div><span class="eyebrow">${esc(SEED.CTX[k].n)} · ${esc(kind)}</span><h2>${esc(source[0])}</h2><p>${esc(source[1])}</p></div><div class="feature-head-actions">${run ? `<span class="badge green">${icon("check")}Updated ${esc(run.time)}</span>` : ""}<button class="btn primary" data-action="capability-run" data-module="${k}" data-feature="${section}">${icon("plus")}${esc(capabilityActionLabel(k, feature.label))}</button></div></section>${capabilityMetrics(k, rows)}<div class="feature-content ${kind === "board" ? "full" : ""}">${capabilityBody(kind, k, rows, r, feature.label)}</div><section class="feature-related"><div class="between"><div><h2>Connected work</h2><p class="small muted">Sources and downstream records that explain this feature.</p></div><a class="link" href="${route("journeys")}">View journeys ${icon("arrow")}</a></div><div class="related-strip">${(r.refs || []).map(resolveRef).filter(Boolean).slice(0, 4).map(recordLinkCard).join("") || rows.filter((x) => x.id !== r.id).slice(0, 3).map(recordLinkCard).join("")}</div></section></div>`;
}

document.addEventListener("click", (event) => {
  const b = event.target.closest('[data-action="capability-run"]');
  if (!b) return;
  const r = workflowRecord(b.dataset.module), feature = capabilityFor(b.dataset.module, b.dataset.feature);
  if (!r || !feature) return;
  state.capabilityRuns ||= {};
  const key = `${state.holding}:${b.dataset.module}:${b.dataset.feature}`;
  state.capabilityRuns[key] = { by: person().name, time: nowLabel, version: r.version };
  if (!moduleReadOnly(b.dataset.module)) {
    wfHistory(r, `Completed a local ${feature.label.toLowerCase()} sample action.`);
    publishRecordUpdate(r, `${feature.label} updated for ${r.title}.`);
  }
  render();
  notify(moduleReadOnly(b.dataset.module) ? `${feature.label} refreshed from its owning records.` : `${feature.label} updated. Related work and history now show the result.`);
});

window.ERPX_CAPABILITIES = { CAPABILITY_MANIFEST, capabilityTabs, capabilityFor, capabilityView };
