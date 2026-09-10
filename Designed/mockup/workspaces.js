/* Reusable workspace compositions: each module chooses its business layout. */
function slug(s) {
  return s
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, "-")
    .replace(/-$/, "");
}
function freshness() {
  const s = state.demoState;
  return ["offline", "degraded", "stale", "denied", "expired"].includes(s)
    ? s[0].toUpperCase() + s.slice(1)
    : "Fresh";
}
function moduleSectionId(k, label) {
  return capabilityTabs(k).find((feature) => feature.label === label)?.id || slug(label);
}
function hasSpecializedSection(k, section) {
  if (k === "sales" && ["quotes", "orders", "fulfillment"].includes(section)) return true;
  if (k === "procurement" && ["requisitions", "purchase-orders", "goods-receipts"].includes(section)) return true;
  if (k === "organization" && ["organization-tree", "departments", "sharing"].includes(section)) return true;
  if (k === "collaboration" && ["following", "topics", "mentions"].includes(section)) return true;
  if (["pipeline", "applications", "board", "schedule", "dispatch-schedule"].includes(section)) return true;
  if (["identity-preview", "content-preview", "listings", "course-catalog", "available-plans", "agent-catalog"].includes(section)) return true;
  if (k === "finance" && ["close-checklist", "banking"].includes(section)) return true;
  if (k === "commercial" && ["subscription", "invoices", "entitlements"].includes(section)) return true;
  if (k === "people_payroll" && section === "payroll-summaries") return true;
  if (k === "platform_administration" && ["backups", "service-health"].includes(section)) return true;
  if (k === "customer_authority" && section === "duplicate-review") return true;
  return ["version-history", "document-versions", "access-history", "maintenance-history", "visit-evidence", "results"].includes(section);
}
function fieldControl(f, value, disabled = false) {
  const attrs = `name="${f.key}" ${disabled ? "disabled" : ""}`;
  let control = "";
  if (f.type === "checkbox")
    control = `<label class="checkbox-label"><input type="checkbox" ${attrs} ${value ? "checked" : ""}>${esc(f.label)}</label>`;
  else if (f.type === "select")
    control = `<label class="field">${esc(f.label)}<select ${attrs}>${f.options.map((o) => `<option ${value === o ? "selected" : ""}>${esc(o)}</option>`).join("")}</select></label>`;
  else if (f.type === "textarea")
    control = `<label class="field full">${esc(f.label)}<textarea rows="3" maxlength="2048" ${attrs}>${esc(value)}</textarea></label>`;
  else
    control = `<label class="field">${esc(f.label)}<input ${attrs} type="${f.type === "number" ? "number" : f.type === "date" ? "date" : "text"}" ${f.type === "number" ? 'step="any"' : 'maxlength="160"'} value="${esc(value)}"></label>`;
  return control;
}
function recordLinkCard(r, extra = "") {
  return `<a class="work-record" href="${r.workflow && taskCanAct(r) && !r.externalWait && !r.optionalAction ? taskRoute(r) : recordRoute(r)}">${symbol(r.module)}<span class="grow"><strong>${esc(r.title)}</strong><small>${esc(r.recordType?.replaceAll("_", " ") || r.subtitle)} · ${esc(r.id)}</small>${extra}</span>${badge(r.status)}${icon("chevron")}</a>`;
}
function attentionReason(r) {
  return r.workflow
    ? taskReason(r)
    : "Open the record to inspect its current state.";
}
function reviewerLabel(r) {
  return r.workflow ? taskPerson(taskOwner(r)) : "Record owner";
}
function workflowHero(r) {
  const c = MODULES[r.module],
    s = nextStep(r),
    steps = flowSteps(r);
  return `<section class="workflow-hero layout-${c.layout}"><div class="eyebrow">${esc(c.kind)} · sample workflow</div><div class="between wrap"><div><h2>${esc(r.title)}</h2><p>${esc(c.focus)}</p></div>${badge(r.status)}</div><div class="workflow-track">${steps.map((x, i) => `<div class="track-step ${i < (r.stage || 0) || r.finished ? "complete" : i === (r.stage || 0) ? "current" : ""}"><span>${i < (r.stage || 0) || r.finished ? "✓" : i + 1}</span><small>${esc(x.action)}</small></div>`).join("") || '<p class="small">Read the combined profile, then follow the source record.</p>'}</div><div class="between wrap"><span class="small muted">${esc(s?.help || "The record and its history remain available.")}</span><a class="btn primary" href="${r.workflow && s ? taskRoute(r) : recordRoute(r)}">${s ? esc(taskAction(r)?.action || "Open task") : "Open details"}${icon("arrow")}</a></div></section>`;
}
function moduleView(k) {
  if (!MODULES[k] || !moduleAllowed(k))
    return empty(
      "Module unavailable",
      "This module is outside your assigned work context.",
      `<a class="btn primary" href="${route("work")}">Back to Work</a>`,
      "lock",
    );
  const c = MODULES[k],
    section = parseRoute().id || slug(c.tabs[0]),
    r = workflowRecord(k),
    all = visibleRecords(k),
    query = state.filter.toLowerCase();
  const rows =
    state.demoState === "empty"
      ? []
      : all.filter(
          (x) =>
            (x.title + " " + x.id + " " + x.status)
              .toLowerCase()
              .includes(query) &&
            (state.statusFilter === "all" || x.status === state.statusFilter),
        );
  const completed = all.filter(
    (x) =>
      x.finished ||
      [
        "Posted",
        "Closed",
        "Active",
        "Completed",
        "Fulfilled",
        "Received",
        "Published",
      ].includes(x.status),
  );
  const featureTabs = capabilityTabs(k),
    additionalFeatureTabs = featureTabs.filter((x) => !c.tabs.some((tab) => slug(tab) === slug(x.label))),
    tabs = ["Overview", ...c.tabs, ...additionalFeatureTabs.map((x) => x.label), "Records", "Activity"],
    primary = c.tabs.slice(0, 3),
    more = [...c.tabs.slice(3), ...additionalFeatureTabs.map((x) => x.label), "Overview", "Records", "Activity"];
  const header = `${heading(esc(SEED.CTX[k].n), "", moduleReadOnly(k) ? '<span class="badge gray">Read-only</span>' : `<button class="btn primary" data-action="wf-new" data-module="${k}">${icon("plus")}${esc(c.create)}</button>`)}<div class="workspace-context"><a class="link" href="${route("modules")}">${icon("back")}All modules</a>${freshness() === "Fresh" ? "" : `<span class="badge amber">${freshness()}</span>`}</div><nav class="section-tabs" aria-label="${esc(SEED.CTX[k].n)} sections">${primary.map((t) => { const id = moduleSectionId(k, t); return `<a href="${route("ctx/" + k + "/" + id)}" class="${section === id ? "active" : ""}" ${section === id ? 'aria-current="page"' : ""}>${esc(t)}</a>`; }).join("")}<details class="section-more"><summary>${more.some((t) => moduleSectionId(k, t) === section) ? esc(more.find((t) => moduleSectionId(k, t) === section) === "Records" ? "All records" : more.find((t) => moduleSectionId(k, t) === section)) : "More"} ${icon("down")}</summary><div>${more.map((t) => { const id = moduleSectionId(k, t); return `<a href="${route("ctx/" + k + "/" + id)}" ${section === id ? 'aria-current="page"' : ""}>${t === "Records" ? "All records" : esc(t)}</a>`; }).join("")}</div></details></nav>`;
  if (!tabs.some((t) => slug(t) === section || capabilitySlug(t) === section))
    return (
      header +
      empty("Section unavailable", "Choose one of the module sections above.")
    );
  if (section === "activity")
    return (
      header +
      `<section class="card card-pad"><h2>Recent record activity</h2>${rows.flatMap((x) => x.history.slice(0, 2).map((h) => `<a class="activity-item" href="${recordRoute(x)}"><strong>${esc(h.by)}</strong><p>${esc(h.text)}</p><small>${esc(x.title)} · ${esc(h.time)}</small></a>`)).join("") || empty("No activity here", "Updates appear when this workspace’s records change.")}</section>`
    );
  if (section === "records") return header + recordCollection(k, rows);
  if (capabilityFor(k, section) && !hasSpecializedSection(k, section))
    return header + capabilityView(k, section, rows);
  if (!r)
    return (
      header +
      empty(
        "No work in this scope",
        "Choose another permitted branch or create a sample record.",
      )
    );
  if (section !== "overview") return header + sectionView(k, section, r, rows);
  const blocked = taskItems("todo", k);
  const metricNames =
    c.layout === "ledger"
      ? ["Records in this company scope", "Completed entries", "Needs review"]
      : c.layout === "pipeline"
        ? ["Open conversations", "Completed handoffs", "Needs attention"]
        : c.layout === "stock"
          ? ["Stock records", "Completed movements", "Needs attention"]
          : ["Records in this scope", "Completed or active", "Needs attention"];
  return (
    header +
    `<div class="compact-metrics">${[
      [metricNames[0], all.length],
      [metricNames[1], completed.length],
      [metricNames[2], blocked.length],
    ]
      .map(
        ([label, value], i) =>
          `<a href="${i === 2 ? route("work") : route("ctx/" + k + "/records")}" ${i === 2 ? `data-action="task-module" data-id="${k}"` : ""}><strong>${value}</strong><span>${label}</span></a>`,
      )
      .join("")}</div><section class="card card-pad"><h2>Needs attention</h2>${
      blocked
        .slice(0, 3)
        .map(
          (x) =>
            `<a class="attention-card" href="${taskRoute(x)}"><div><strong>${esc(taskAction(x)?.action || "Open task")}</strong><p>${esc(x.title)}</p><small>${esc(taskDue(x))} · ${esc(reviewerLabel(x))}</small></div>${icon("chevron")}</a>`,
        )
        .join("") || '<p class="small muted">You’re all caught up.</p>'
    }</section><section class="card card-pad module-recent"><div class="between"><h2>Recent records</h2><a class="link" href="${route("ctx/" + k + "/records")}">View all</a></div>${rows
      .slice(0, 4)
      .map((x) => recordLinkCard(x))
      .join("")}</section>`
  );
}
function recordCollection(k, rows) {
  return `<form class="filter-row" data-form="record-filter"><input name="query" aria-label="Search records" placeholder="Search names, IDs or statuses…" value="${esc(state.filter)}"><select name="status" aria-label="Filter by status"><option value="all">All statuses</option>${[...new Set(visibleRecords(k).map((r) => r.status))].map((s) => `<option ${s === state.statusFilter ? "selected" : ""}>${esc(s)}</option>`).join("")}</select></form>${rows.length ? `<section class="card card-pad record-collection">${rows.map((r) => recordLinkCard(r)).join("")}<p class="tiny muted">${rows.length} records · ${esc(scopeLabel())}</p></section>` : empty("No matching records", "Try a different search or status filter.")}`;
}
function sectionView(k, section, r, rows) {
  const c = MODULES[k],
    title = c.tabs.find((x) => slug(x) === section) || section;
  if (k === "sales" && ["quotes", "orders", "fulfillment"].includes(section)) {
    const selected = rows.filter((x) =>
      section === "quotes"
        ? x.recordType !== "sales_order"
        : x.recordType === "sales_order",
    );
    return `<section class="card card-pad"><h2>${esc(title)}</h2><p class="small muted">${section === "quotes" ? "Customer proposals with payment terms and acceptance history." : "Confirmed orders have their own fulfillment lifecycle and linked source quote."}</p></section>${recordCollection(k, selected)}`;
  }
  if (
    k === "procurement" &&
    ["requisitions", "purchase-orders", "goods-receipts"].includes(section)
  ) {
    const type =
      section === "purchase-orders"
        ? "purchase_order"
        : section === "goods-receipts"
          ? "goods_receipt"
          : "requisition";
    return `<section class="card card-pad"><h2>${esc(title)}</h2><p class="small muted">${type === "goods_receipt" ? "Check quantities and evidence before accepting a supplier delivery." : type === "purchase_order" ? "Supplier orders retain their approved requisition and receipt links." : "Discuss the need and requested items before approval."}</p></section>${recordCollection(
      k,
      rows.filter(
        (x) => x.recordType === type || (!x.workflow && type === "requisition"),
      ),
    )}`;
  }
  if (section === "organization-tree")
    return organizationTree() + workflowHero(r);
  if (section === "following")
    return `<div class="stack">${topicDirectory()}${
      rows
        .filter((x) => isFollowed("record", x.module + "/" + x.id))
        .map((x) => recordLinkCard(x))
        .join("") ||
      empty(
        "Follow your working threads",
        "Use Follow on a record to keep it here.",
      )
    }</div>`;
  if (section === "topics") return topicDirectory();
  if (section === "mentions")
    return `<section class="card card-pad"><h2>Your mentions</h2>${
      mentionItems()
        .map((x) => recordLinkCard(x))
        .join("") ||
      empty(
        "No mentions yet",
        "Mention an assigned colleague in a record discussion to create a scoped inbox item.",
      )
    }</section>`;
  if (["pipeline", "applications", "board"].includes(section))
    return `<div class="workflow-board">${flowSteps(r)
      .slice(0, 4)
      .map(
        (s) =>
          `<section><h3>${esc(s.status)}</h3>${
            rows
              .filter((x) => x.status === s.status)
              .map((x) => recordLinkCard(x))
              .join("") ||
            '<p class="small muted">No records in this stage.</p>'
          }</section>`,
      )
      .join("")}</div>`;
  if (["schedule", "dispatch-schedule"].includes(section))
    return `<section class="card card-pad"><h2>${esc(title)}</h2><p class="small muted">Sample appointments in your current business unit.</p>${rows
      .filter((x) => x.workflow)
      .map(
        (x) =>
          `<a class="schedule-row" href="${recordRoute(x)}"><div class="date-block"><strong>${esc(x.fields.scheduled?.slice(-2) || "06")}</strong><small>SEP</small></div><span><strong>${esc(x.title)}</strong><small>${esc(x.fields.technician || "Assigned team")} · ${esc(branchName(x.branch))}</small></span>${badge(x.status)}</a>`,
      )
      .join("")}</section>`;
  if (
    [
      "identity-preview",
      "content-preview",
      "listings",
      "course-catalog",
      "available-plans",
      "agent-catalog",
    ].includes(section)
  )
    return `<div class="stack">${contextPanel(r, true)}${recordCollection(
      k,
      rows.filter((x) => x.workflow),
    )}</div>`;
  if (section === "close-checklist")
    return (
      financePanel(r) +
      recordCollection(
        k,
        rows.filter(
          (x) =>
            x.workflow || /period|journal/i.test(x.subtitle + " " + x.title),
        ),
      )
    );
  if (section === "banking")
    return `<section class="card card-pad"><h2>Bank statement preview</h2><p class="small muted">Local sample statement. Banking connections and file uploads are not active.</p><div class="statement-row"><span>Sep 04 · Riverstone settlement</span><strong>${money(2400)}</strong><span class="badge gray">Match proposed</span></div><div class="statement-row"><span>Sep 05 · Cedar goods receipt</span><strong>${money(-2400)}</strong><span class="badge gray">Review required</span></div><a class="btn primary" href="${recordRoute(r)}">Review journal and reconciliation ${icon("arrow")}</a></section>`;
  if (
    section === "subscription" ||
    section === "invoices" ||
    section === "entitlements"
  )
    return `<section class="card card-pad"><h2>${esc(title)}</h2><div class="notice">${icon("shield")}Provider status · read-only sample</div><div class="summary-facts"><div><small>Plan</small><strong>Enterprise annual</strong></div><div><small>Seats allocated</small><strong>${state.holding === "northstar" ? 24 : 80}</strong></div><div><small>Billing status</small><strong>Active · sample mirror</strong></div></div><p class="small muted">Plan access follows the sample provider status. This view cannot charge, refund or change a subscription.</p>${rows
      .filter((x) => !x.workflow)
      .map((x) => recordLinkCard(x))
      .join("")}</section>`;
  if (section === "payroll-summaries")
    return `<section class="card card-pad"><h2>Payroll summaries</h2><p class="small muted">Restricted to customer owner and Finance. Payroll amounts never appear in the general feed.</p>${
      ["owner", "finance"].includes(state.role)
        ? `<div class="summary-facts"><div><small>Current period</small><strong>September</strong></div><div><small>Preparation</small><strong>Awaiting review</strong></div></div>${
            rows
              .filter((x) => /payroll|pay|run/i.test(x.title + " " + x.id))
              .map((x) => recordLinkCard(x))
              .join("") || "<p>No payroll run in this scope.</p>"
          }`
        : empty(
            "Summary unavailable",
            "Your assigned role cannot view payroll amounts.",
          )
    }</section>`;
  if (section === "departments" || section === "sharing")
    return `<section class="card card-pad"><h2>${esc(title)}</h2><div class="summary-facts">${["Sales", "Operations", "Finance", "People", "Customer service"].map((x) => `<div><small>${section === "sharing" ? "Audience circle" : "Department"}</small><strong>${x}</strong><span class="tiny muted">${["Finance", "People"].includes(x) ? "Shared across companies" : "Works within assigned branches"}</span></div>`).join("")}</div><div class="notice">An audience never grants access. Branch and role restrictions still apply.</div><button class="btn" data-action="workspace">Inspect your work context</button></section>`;
  if (section === "backups" || section === "service-health")
    return `<section class="card card-pad"><h2>${esc(title)}</h2><div class="service-checks"><div>${icon("check")}Last sample recovery check passed</div><div>${icon("shield")}Customer business content is isolated</div><div>${icon("clock")}Sample checkpoint · today 08:30</div></div>${recordLinkCard(r)}</section>`;
  if (section === "duplicate-review")
    return `<section class="card card-pad"><h2>Compare profiles</h2><div class="comparison-grid"><div><small>Retained profile</small><h3>${esc(r.fields.primary)}</h3><p>Customer authority binding · verified registration</p></div><div><small>Source profile</small><h3>${esc(r.fields.duplicate)}</h3><p>Same registration · history retained after merge</p></div></div>${recordLinkCard(r)}</section>`;
  if (
    section === "version-history" ||
    section === "document-versions" ||
    section === "access-history" ||
    section === "maintenance-history" ||
    section === "visit-evidence" ||
    section === "results"
  )
    return `<section class="card card-pad"><h2>${esc(title)}</h2>${r.history.map((x) => `<div class="activity-item"><strong>${esc(x.by)}</strong><p>${esc(x.text)}</p><small>${esc(x.time)}</small></div>`).join("")}${r.evidence.map((x) => `<a class="reference" href="${x.asset}" target="_blank">${icon("file")}${esc(x.name)} · ${esc(x.by)}</a>`).join("")}${recordLinkCard(r)}</section>`;
  return `<div class="stack"><section class="card card-pad"><div class="between"><h2>${esc(title)}</h2><span class="badge gray">${esc(scopeLabel())}</span></div>${contextPanel(r, false, true)}<a class="btn soft" href="${recordRoute(r)}">Inspect ${esc(c.kind.toLowerCase())}${icon("arrow")}</a></section>${recordCollection(
    k,
    rows.filter((x) => !x.workflow || !x.parentId),
  )}</div>`;
}
function financePanel(r) {
  const v = r.fields,
    period = state.periods[r.holding + ":" + primaryCompany(r)] || "Open";
  return `<section class="card card-pad"><div class="between"><h2>Journal & period health</h2>${badge(period)}</div><div class="table-wrap" tabindex="0" role="region" aria-label="Journal debits and credits; scroll horizontally on narrow screens"><table><thead><tr><th>Account</th><th>Debit</th><th>Credit</th></tr></thead><tbody><tr><td>1200 · Inventory</td><td>${money(v.debit || 2400)}</td><td>—</td></tr><tr><td>2100 · Accounts payable</td><td>—</td><td>${money(v.credit || 2400)}</td></tr></tbody></table></div><div class="summary-facts"><div><small>Balance</small><strong>${v.debit === v.credit ? "Balanced" : "Needs correction"}</strong></div><div><small>Reconciliation</small><strong>${v.reconciled ? "Checked" : "Needs review"}</strong></div><div><small>Close reviewer</small><strong>Different person required</strong></div></div><p class="tiny muted">Company ${esc(primaryCompany(r))} · opening a closed period is outside this sample workflow.</p></section>`;
}
function contextPanel(r, large = false, embedded = false) {
  const c = MODULES[r.module],
    v = wfData(r);
  let body = "";
  if (r.module === "finance") return financePanel(r);
  if (c.layout === "studio" || r.module === "ecommerce")
    body = `<div class="brand-preview ${large ? "large" : ""}"><div class="eyebrow">${r.module === "brand" ? "IDENTITY PREVIEW" : "CUSTOMER PREVIEW"}</div><div class="preview-mark">${icon(r.module === "brand" ? "briefcase" : "cart")}</div><h2>${esc(v.plan || r.title)}</h2><p>${esc(v.notes || c.focus)}</p>${v.price ? `<strong>${money(v.price)}</strong>` : ""}</div>`;
  else if (r.module === "pos")
    body = `<h2>Your sample basket</h2><div class="product-row"><img src="assets/store.svg" alt="Sample store illustration"><div class="grow"><strong>Everyday canvas tote</strong><p>${esc(v.quantity)} × ${money(v.unitPrice)}</p></div><strong>${money(v.quantity * v.unitPrice)}</strong></div><div class="summary-facts"><div><small>Tender</small><strong>${money(v.tender)}</strong></div><div><small>Change</small><strong>${money(Math.max(0, v.tender - v.quantity * v.unitPrice))}</strong></div><div><small>Receipt state</small><strong>${esc(r.status)}</strong></div></div><a class="btn soft" href="${recordRoute(r)}">${r.status === "Open" ? "Edit basket" : "Open receipt"}${icon("arrow")}</a>`;
  else if (r.module === "planning")
    body = `<h2>Compare the replenishment options</h2><div class="comparison-grid"><div><span class="badge">Base scenario</span><strong class="big-number">${Math.max(0, v.demand - v.available)}</strong><p>Units required · ${esc(v.demand)} demand − ${esc(v.available)} available</p></div><div><span class="badge gray">With 10% buffer</span><strong class="big-number">${Math.max(0, Math.ceil(v.demand * 1.1) - v.available)}</strong><p>Compare before approving the base recommendation.</p></div></div>`;
  else if (r.module === "inventory")
    body = `<h2>Availability after receiving</h2><div class="stock-visual"><div><strong>${esc(v.expected)}</strong><small>Expected</small></div>${icon("arrow")}<div><strong>${r.status === "Received" ? esc(v.quantity) : 0}</strong><small>Received</small></div>${icon("arrow")}<div><strong>${80 + (r.status === "Received" ? Number(v.quantity) : 0)}</strong><small>Available units</small></div></div><p class="small muted">${esc(v.sku)} · opening sample availability: 80 units</p>`;
  else if (r.module === "quality")
    body = `<h2>Quality release gate</h2><div class="quality-gauge ${v.result === "Pass" && v.reading <= 5 ? "pass" : ""}"><strong>${esc(v.reading)}°C</strong><span>Safe range 0–5°C</span>${badge(r.status)}</div><p class="small muted">${r.status === "Released" ? "The sample gate has been released." : "Dependent work stays blocked until a passing recheck is reviewed."}</p>`;
  else if (r.module === "learning")
    body = `<div class="learning-preview"><img src="assets/warehouse.svg" alt="Warehouse lesson illustration"><div><h2>Receiving essentials</h2><p>A 12-second sample lesson, with captions.</p><button class="btn primary" data-action="short" data-id="receive">${icon("play")}Watch lesson</button></div></div>`;
  else if (r.module === "organization") return organizationTree();
  else if (r.module === "customer_360")
    body = `<div class="customer-profile-head">${avatar(r.title, "RM", "blue")}<div><h2>${esc(r.title)}</h2><p>Retail customer · source records remain in their owning modules</p></div></div><div class="summary-facts">${r.refs
      .map(resolveRef)
      .filter(Boolean)
      .map(
        (x) =>
          `<a href="${recordRoute(x)}"><small>${esc(SEED.CTX[x.module].n)}</small><strong>${esc(x.status)}</strong><span>${esc(x.title)}</span></a>`,
      )
      .join("")}</div>`;
  else if (c.layout === "service")
    body = `<h2>${r.module === "agent" ? "Sample capability and output" : "Service checklist"}</h2><div class="service-checks"><div>${icon("shield")}${r.module === "agent" ? "Reads only the sources available to the requesting person" : "No customer business data in this service view"}</div><div>${icon("file")}${esc(v.notes)}</div><div>${icon("check")}${r.module === "agent" ? "Output remains a proposed draft until human review" : "Review, progress and outcome stay visible"}</div></div>${r.status === "Output ready" ? `<blockquote class="agent-output"><span class="badge gray">Proposed by Handover assistant · sample</span><p>Receiving is ready for the next shift. Inspect the linked quality check before releasing the stock.</p><button class="btn" data-action="wf-agent-reject" data-module="agent" data-record="${esc(r.id)}">Reject output</button></blockquote>` : ""}`;
  else if (c.layout === "pipeline" || c.layout === "project")
    body = `<h2>${c.layout === "project" ? "Milestone progress" : "Pipeline progress"}</h2><div class="mini-pipeline">${flowSteps(
      r,
    )
      .map(
        (s, i) =>
          `<span class="${(r.stage || 0) >= i ? "active" : ""}">${esc(s.status)}</span>`,
      )
      .join("")}</div><p class="small muted">${esc(v.notes)}</p>`;
  else
    body = `<h2>${esc(c.tabs[0])} at a glance</h2><div class="summary-facts">${
      recordFields(r)
        .filter((f) => f.type !== "textarea")
        .slice(0, 3)
        .map(
          (f) =>
            `<div><small>${esc(f.label)}</small><strong>${esc(f.type === "checkbox" ? (v[f.key] ? "Verified" : "Needs review") : v[f.key])}</strong></div>`,
        )
        .join("") ||
      `<div><small>Current status</small><strong>${esc(r.status)}</strong></div>`
    }</div><p class="small muted">${esc(v.notes || c.focus)}</p>`;
  return embedded ? body : `<section class="card card-pad">${body}</section>`;
}
function recordView(k, id) {
  const r = findRecord(k, id);
  if (!r || !r.workflow) return legacyRecordView(k, id);
  return taskView(k, id, "details");
}
function discussionPanel(r) {
  const comments = r.comments.filter((c) => referencesVisible(c.refs));
  return `<section class="card card-pad"><div class="between section-head"><div><h2>Conversation</h2><p class="small muted">The same record thread appears wherever this work is linked.</p></div><span class="small muted">${comments.length} entries</span></div>${comments.map((c, i) => `<article class="discussion-entry" id="message-${esc(c.id || i)}">${avatar(c.by)}<div class="grow"><strong>${esc(c.by)}</strong><span class="badge gray">${c.visibility === "internal" ? "Internal note" : "Discussion"}</span><p>${c.tombstone ? "This comment was moderated. Its history is retained." : esc(c.text)}</p>${(c.refs || []).map(referenceChip).join("")}${attachmentList(c.attachments || [])}${!c.tombstone && ["owner", "manager"].includes(role().type) ? `<button class="link tiny" data-action="cx-moderate" data-module="${r.module}" data-record="${esc(r.id)}" data-index="${r.comments.indexOf(c)}">Moderate comment ${i + 1}</button>` : ""}</div></article>`).join("") || '<p class="small muted">Keep useful context close to the work.</p>'}<form class="discussion-form" data-form="wf-comment" data-module="${r.module}" data-record="${esc(r.id)}"><textarea name="text" maxlength="1500" rows="2" aria-label="Message this record conversation" placeholder="Ask a question, @mention a colleague or link work…" required></textarea>${attachmentComposer(r)}<div class="between">${r.module === "support" ? '<label class="field">Reply visibility<select name="visibility"><option value="internal">Internal note · team only</option><option value="public">Customer reply · sample only</option></select></label>' : '<span class="tiny muted">Current work context</span>'}<button type="submit" class="btn soft">Send message</button></div></form></section>`;
}

window.ERPX_WORKSPACES = {
  moduleView,
  recordView,
  fieldControl,
  attentionReason,
  discussionPanel,
};
