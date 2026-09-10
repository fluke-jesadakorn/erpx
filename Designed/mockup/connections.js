/* Scoped social discovery, reference editors and connected journeys. */
function isFollowed(type, id) {
  return state.targets.includes(actorKey(type + ":" + id));
}
function followButton(type, id) {
  return `<button class="btn ${isFollowed(type, id) ? "soft" : ""} follow-control" data-action="cx-follow" data-type="${type}" data-target="${esc(id)}" aria-pressed="${isFollowed(type, id)}">${icon(isFollowed(type, id) ? "check" : "plus")}${isFollowed(type, id) ? "Following" : "Follow"}</button>`;
}
function availablePeople() {
  return Object.entries(personas).filter(([, p]) =>
    p.assignments.some(
      (a) =>
        a.holding === state.holding &&
        (state.branch === "all" ||
          a.branches.includes("all") ||
          a.branches.includes(state.branch)),
    ),
  );
}
function principalVisible(ref) {
  return ref.type === "person"
    ? availablePeople().some(([id]) => id === ref.id)
    : ref.type === "group"
      ? moduleAllowed(ref.id)
      : ref.type === "pool"
        ? moduleAllowed("agent")
        : false;
}
function referenceChip(ref) {
  const target = resolveRef(ref),
    principal = principalVisible(ref);
  if (!target && !principal)
    return '<span class="ref-chip unavailable">Unavailable reference</span>';
  const label = target
    ? target.id
    : ref.type === "person"
      ? personas[ref.id].name
      : ref.type === "group"
        ? SEED.CTX[ref.id].n
        : "Approved assistant pool";
  return target
    ? `<a class="ref-chip" href="${recordRoute(target)}">${icon("link")}@${esc(label)}<small>${esc(ref.relation || "related to")}</small></a>`
    : `<span class="ref-chip">${icon("users")}@${esc(label)}</span>`;
}
function referencePanel(r) {
  const backs = visibleRecords().filter(
    (x) =>
      x.id !== r.id &&
      (x.refs || []).some(
        (ref) =>
          ref.type === "record" && ref.module === r.module && ref.id === r.id,
      ),
  );
  return `<section class="card card-pad reference-panel" data-host-module="${r.module}" data-host-record="${esc(r.id)}"><div class="between"><h2>Connected work</h2>${!recordReadOnly(r) ? `<button class="icon-btn" data-action="cx-add-ref" data-module="${r.module}" data-record="${esc(r.id)}" aria-label="Add record reference">${icon("plus")}</button>` : ""}</div><p class="tiny muted">Ordered links · ${r.refs?.length || 0}/50</p><div class="ordered-refs">${(r.refs || []).map((ref, i) => `<div class="ordered-ref">${referenceChip(ref)}${!recordReadOnly(r) ? `<div class="ref-actions"><button class="icon-btn" data-action="cx-move-ref" data-module="${r.module}" data-record="${esc(r.id)}" data-index="${i}" data-direction="-1" aria-label="Move reference ${i + 1} up" ${i === 0 ? "disabled" : ""}>↑</button><button class="icon-btn" data-action="cx-move-ref" data-module="${r.module}" data-record="${esc(r.id)}" data-index="${i}" data-direction="1" aria-label="Move reference ${i + 1} down" ${i === r.refs.length - 1 ? "disabled" : ""}>↓</button><button class="icon-btn" data-action="cx-remove-ref" data-module="${r.module}" data-record="${esc(r.id)}" data-index="${i}" aria-label="Remove reference ${i + 1}">×</button></div>` : ""}</div>`).join("") || '<p class="small muted">Link work to explain the next handoff.</p>'}</div><div class="line"></div><h3>Mentioned in</h3>${
    backs
      .slice(0, 50)
      .map(
        (x) =>
          `<a class="reference" href="${recordRoute(x)}">${icon("link")}<span>${esc(x.title)}<small>${esc(SEED.CTX[x.module].n)} · ${esc(x.id)}</small></span></a>`,
      )
      .join("") || '<p class="small muted">No accessible backlinks yet.</p>'
  }</section>`;
}
function editorKey(id) {
  return (
    ctxKey() +
    ":" +
    id +
    (id === "post-text" ? ":" + (draft?.mode || "post") : "")
  );
}
function editorRefsFor(id) {
  return clone(state.editorRefs[editorKey(id)] || []);
}
function editorId(el) {
  const f = el.closest("form"),
    p = parseRoute();
  return (
    "editor-" +
    (f?.dataset.form || "field") +
    "-" +
    (f?.dataset.id || f?.dataset.record || p.id || p.page) +
    "-" +
    (el.name || "text")
  );
}
function enhanceEditors(root = document) {
  const els = root.querySelectorAll(
    'textarea:not([disabled]),[data-form="comment"] input[name="text"],[data-form="record-comment"] input[name="text"],#record-form input[type="text"],[data-form="wf-save"] input[type="text"],[data-form="task-draft"] input[type="text"]',
  );
  for (const el of els) {
    if (el.dataset.enhanced || el.id === "wf-decision") continue;
    el.dataset.enhanced = "true";
    el.id ||= editorId(el);
    const routeContext = parseRoute(),
      host = ["rec", "tasks"].includes(routeContext.page)
        ? findRecord(routeContext.key, routeContext.id)
        : null;
    if (!state.editorRefs[editorKey(el.id)] && host?.fieldRefs?.[el.name])
      state.editorRefs[editorKey(el.id)] = clone(host.fieldRefs[el.name]);
    const tools = document.createElement("div");
    tools.className = "editor-tools";
    tools.dataset.editor = el.id;
    tools.innerHTML = `<div class="editor-chips">${editorRefsFor(el.id)
      .map(
        (ref, i) =>
          `${referenceChip(ref)}<button type="button" class="chip-remove" data-action="cx-remove-chip" data-editor="${esc(el.id)}" data-index="${i}" aria-label="Remove mention ${i + 1}">×</button>`,
      )
      .join(
        "",
      )}</div><button type="button" class="link tiny" data-action="cx-picker" data-editor="${esc(el.id)}" aria-label="Link a record or person">@</button>`;
    el.insertAdjacentElement("afterend", tools);
  }
}
function pickerTargets(q) {
  q = q.toLowerCase().replace(/^@/, "");
  return [
    ...visibleRecords()
      .filter((r) =>
        (r.module + "/" + r.id + " " + r.title).toLowerCase().includes(q),
      )
      .map((r) => ({
        ref: refOf(r),
        name: r.title,
        sub: "@" + r.module + "/" + r.id,
      })),
    ...availablePeople()
      .filter(([, p]) => p.name.toLowerCase().includes(q) || q === "user")
      .map(([id, p]) => ({
        ref: { type: "person", holding: state.holding, id },
        name: p.name,
        sub: "Person · current workspace",
      })),
    ...role()
      .modules.filter((k) =>
        (SEED.CTX[k].n + " group").toLowerCase().includes(q),
      )
      .map((k) => ({
        ref: { type: "group", holding: state.holding, id: k },
        name: SEED.CTX[k].n,
        sub: "Module group",
      })),
    ...(moduleAllowed("agent") && "approved assistant pool".includes(q)
      ? [
          {
            ref: { type: "pool", holding: state.holding, id: "approved" },
            name: "Approved assistant pool",
            sub: "Sample approved pool",
          },
        ]
      : []),
  ].slice(0, 30);
}
let activePicker = null;
function openReferencePicker(button) {
  document.querySelector("#reference-picker")?.remove();
  activePicker = {
    context: ctxKey(),
    editor: button.dataset.editor,
    module: button.dataset.module,
    record: button.dataset.record,
  };
  const target =
    button.closest(".editor-tools,.reference-panel") || button.parentElement;
  const div = document.createElement("div");
  div.id = "reference-picker";
  div.className = "reference-picker";
  div.setAttribute("role", "dialog");
  div.setAttribute("aria-label", "Link work or mention a person");
  div.innerHTML = `<div class="between"><strong>Link work or mention a person</strong><button type="button" class="icon-btn" data-action="cx-close-picker" aria-label="Close reference picker">×</button></div><input id="cx-query" aria-label="Find a permitted reference" placeholder="Search @record, person or group…"><label class="tiny muted">Relationship<select id="cx-relation"><option>related to</option><option>depends on</option><option>blocks</option><option>billed by</option><option>fulfills</option><option>authored with</option></select></label><div id="cx-results" role="listbox" aria-label="Permitted references"></div>`;
  target.appendChild(div);
  renderPicker("");
  div.querySelector("input").focus();
}
function renderPicker(q) {
  if (!$("cx-results") || !activePicker || activePicker.context !== ctxKey())
    return;
  const choices = pickerTargets(q);
  $("cx-results").innerHTML =
    choices
      .map(
        (x, i) =>
          `<button type="button" class="search-result" role="option" aria-selected="false" data-action="cx-insert-ref" data-index="${i}"><span><strong>${esc(x.name)}</strong><small>${esc(x.sub)}</small></span>${icon("plus")}</button>`,
      )
      .join("") || '<p class="small muted">No permitted references match.</p>';
  activePicker.choices = choices;
}
function visibleTopics() {
  const tags = new Map();
  const add = (tag, r) => {
    if (!tag.startsWith("#")) tag = "#" + tag;
    const scope = ["#incident", "#platform", "#period-gate"].includes(tag)
      ? "System reserved"
      : r?.branch &&
          r.branch !== "all" &&
          [
            "#stock",
            "#shortage",
            "#shift",
            "#team",
            "#field",
            "#visit",
          ].includes(tag)
        ? "Branch / local"
        : "Holding";
    if (!tags.has(tag)) tags.set(tag, { tag, scope, count: 0 });
    tags.get(tag).count++;
  };
  for (const r of visibleRecords())
    for (const tag of SEED.CTX[r.module]?.tags || []) add(tag, r);
  for (const p of visiblePosts()) for (const tag of p.tags || []) add(tag, p);
  for (const t of state.customTopics.filter(
    (t) =>
      t.holding === state.holding &&
      (t.branch === "all" || t.branch === state.branch),
  ))
    if (!tags.has(t.tag)) tags.set(t.tag, { ...t, count: 0 });
  return [...tags.values()];
}
function composerTags() {
  const text = $("post-text")?.value || "";
  return [...new Set(text.match(/#[a-z][a-z0-9-]*/gi) || [])].filter((tag) =>
    visibleTopics().some(
      (t) =>
        t.tag.toLowerCase() === tag.toLowerCase() &&
        t.scope !== "System reserved",
    ),
  );
}
function topicDirectory() {
  return `<section class="card card-pad"><div class="section-head"><h2>Topics in your workspace</h2><p class="small muted">Local, Holding and system topics follow the same access rules as their records.</p></div><div class="topic-grid">${visibleTopics()
    .map(
      (t) =>
        `<article><a href="${route("tags/" + encodeURIComponent(t.tag.slice(1)))}"><strong>${esc(t.tag)}</strong></a><small>${esc(t.scope)} · ${t.count} visible references</small>${followButton("tag", t.tag)}</article>`,
    )
    .join("")}</div></section>`;
}
function topicView(tag) {
  const t = visibleTopics().find(
    (t) => t.tag === "#" + decodeURIComponent(tag || ""),
  );
  if (!t)
    return empty(
      "Topic unavailable",
      "No topic details are available in this work context.",
    );
  const rs = visibleRecords().filter((r) =>
      SEED.CTX[r.module]?.tags?.includes(t.tag),
    ),
    ps = visiblePosts().filter((p) => p.tags?.includes(t.tag));
  return `${heading(esc(t.tag), `${t.scope} · ${scopeLabel()}`, followButton("tag", t.tag))}<div class="stack">${ps.map(postCard).join("")}<section class="card card-pad"><h2>Connected records</h2>${rs.map((r) => recordLinkCard(r)).join("") || '<p class="small muted">No records in this topic yet.</p>'}</section></div>`;
}
function journeyTaskRecord(k, j) {
  const root = workflowRecord(j.modules[0]);
  const seen = new Set();
  let current = root;
  while (current && !seen.has(current.module + "/" + current.id)) {
    seen.add(current.module + "/" + current.id);
    if (current.module === k) return current;
    current = current.taskHandoffRef
      ? resolveRef(current.taskHandoffRef)
      : null;
  }
  return workflowRecord(k);
}
function journeyView(id) {
  const j = JOURNEYS[id];
  if (!j)
    return `${heading("Connected journeys", "Follow work across departments without losing its context.")}<div class="journey-grid">${Object.entries(
      JOURNEYS,
    )
      .filter(([, x]) => x.modules.some(moduleAllowed))
      .map(
        ([id, x]) =>
          `<a class="card card-pad" href="${route("journeys/" + id)}"><span class="eyebrow">${x.modules.length} connected modules</span><h2>${esc(x.name)}</h2><p>${esc(x.description)}</p>${icon("arrow")}</a>`,
      )
      .join("")}</div>`;
  return `${heading(esc(j.name), esc(j.description))}<a class="link" href="${route("work")}">${icon("back")}Back to Work</a><div class="journey-chain">${j.modules
    .map((k, i) => {
      const r = journeyTaskRecord(k, j);
      return `<section class="journey-node"><span class="journey-number">${i + 1}</span>${
        r
          ? `${symbol(k)}<h2>${esc(SEED.CTX[k].n)}</h2><p>${esc(r.title)}</p>${badge(r.status)}<a class="btn primary" href="${recordRoute(r)}">Open record ${icon("arrow")}</a>${r.refs
              .filter((ref) => resolveRef(ref))
              .slice(0, 2)
              .map(referenceChip)
              .join("")}`
          : "<h2>Unavailable step</h2><p>Your assigned context cannot preview this step.</p>"
      }</section>${i < j.modules.length - 1 ? `<div class="journey-edge">${icon("arrow")}<span>${r ? esc(j.edges[i]) : "Connected work"}</span></div>` : ""}`;
    })
    .join(
      "",
    )}</div><div class="notice">These links explain the handoffs. Each owning module reviews its own changes; progressing one record does not approve another.</div>`;
}
function postDetailView(id) {
  const p = visiblePosts().find((x) => x.id === id);
  if (!p)
    return empty(
      "Post unavailable",
      "This post cannot be previewed in the current context.",
    );
  if (p.kind === "record" && p.ref) return recordView(p.ref.module, p.ref.id);
  return `${heading("Team conversation", "Scoped to your current workspace")}${postCard(p)}`;
}
function mentionItems() {
  return visibleRecords().filter((r) =>
    r.comments.some(
      (c) =>
        !c.tombstone &&
        (c.refs || []).some(
          (x) => x.type === "person" && x.id === state.persona,
        ) &&
        referencesVisible(c.refs),
    ),
  );
}
function enhanceSurface() {
  enhanceEditors();
}
function enrichDialog() {
  enhanceEditors($("dialog"));
  if ($("post-form") && !$("composer-topics"))
    $("post-form").insertAdjacentHTML(
      "beforeend",
      `<div id="composer-topics" class="topic-suggestions"><span class="tiny muted">Add a topic</span>${visibleTopics()
        .filter((t) => t.scope !== "System reserved")
        .slice(0, 8)
        .map(
          (t) =>
            `<button type="button" class="tag-button" data-action="cx-compose-tag" data-target="${esc(t.tag)}">${esc(t.tag)}</button>`,
        )
        .join("")}</div>`,
    );
  const grid = document.querySelector(".demo-state-grid");
  if (grid && !grid.querySelector('[data-id="offline"]'))
    grid.insertAdjacentHTML(
      "beforeend",
      [
        ["offline", "Offline"],
        ["degraded", "Degraded sources"],
        ["source-conflict", "Source conflict"],
        ["expired", "Expired assignment"],
        ["prerequisite", "Business prerequisite"],
        ["handoff-failure", "Handoff preparation fails"],
      ]
        .map(
          ([id, label]) =>
            `<button class="btn" data-action="scenario" data-id="${id}">${label}</button>`,
        )
        .join(""),
    );
  taskDemoControls();
}
document.addEventListener("click", (event) => {
  const b = event.target.closest("[data-action]");
  if (!b) return;
  const a = b.dataset.action;
  const r =
    b.dataset.module && b.dataset.record
      ? findRecord(b.dataset.module, b.dataset.record)
      : null;
  if (a === "cx-follow") {
    const type = b.dataset.type,
      id = b.dataset.target;
    const allowed =
      type === "record"
        ? !!findRecord(...id.split("/"))
        : type === "tag"
          ? visibleTopics().some((t) => t.tag === id)
          : visiblePosts().some((p) => p.by === id) ||
            availablePeople().some(([, p]) => p.name === id);
    if (!allowed) return;
    toggle(state.targets, actorKey(type + ":" + id));
    render();
    notify(
      isFollowed(type, id)
        ? "Following. Your access stays the same."
        : "Unfollowed.",
    );
  }
  if (a === "cx-picker" || a === "cx-add-ref") openReferencePicker(b);
  if (a === "cx-close-picker") {
    document.querySelector("#reference-picker")?.remove();
    activePicker = null;
  }
  if (a === "cx-insert-ref") {
    const p = activePicker;
    if (!p || p.context !== ctxKey()) return;
    const chosen = p.choices[Number(b.dataset.index)];
    if (!chosen) return;
    const ref = { ...chosen.ref, relation: $("cx-relation").value };
    if (
      (ref.type === "record" && !resolveRef(ref)) ||
      (ref.type !== "record" && !principalVisible(ref))
    )
      return;
    const host = p.record ? findRecord(p.module, p.record) : null,
      refs = host
        ? (host.refs ||= [])
        : (state.editorRefs[editorKey(p.editor)] ||= []);
    if (host && !wfPermission(host)) return;
    if (refs.length >= 50) {
      notify("This host already has 50 references. Remove one first.");
      return;
    }
    if (
      !refs.some(
        (x) =>
          x.type === ref.type && x.id === ref.id && x.module === ref.module,
      )
    ) {
      refs.push(ref);
      if (host) wfHistory(host, "Added a " + ref.relation + " reference.");
    }
    document.querySelector("#reference-picker")?.remove();
    activePicker = null;
    if (host) render();
    else {
      const el = $(p.editor);
      el?.closest("form")
        ?.querySelectorAll(".editor-tools")
        .forEach((x) => x.remove());
      el?.closest("form")
        ?.querySelectorAll("[data-enhanced]")
        .forEach((x) => delete x.dataset.enhanced);
      enhanceEditors(el?.closest("form") || document);
      el?.focus();
    }
  }
  if (a === "cx-remove-chip") {
    const refs = state.editorRefs[editorKey(b.dataset.editor)] || [];
    refs.splice(Number(b.dataset.index), 1);
    const el = $(b.dataset.editor);
    el?.nextElementSibling?.remove();
    if (el) delete el.dataset.enhanced;
    enhanceEditors(el?.closest("form") || document);
  }
  if (["cx-remove-ref", "cx-move-ref"].includes(a) && r && wfPermission(r)) {
    const i = Number(b.dataset.index);
    if (a === "cx-remove-ref") r.refs.splice(i, 1);
    else {
      const n = i + Number(b.dataset.direction);
      if (n >= 0 && n < r.refs.length)
        [r.refs[n], r.refs[i]] = [r.refs[i], r.refs[n]];
    }
    wfHistory(r, "Updated the ordered reference list.");
    render();
  }
  if (a === "cx-moderate" && r && ["owner", "manager"].includes(role().type)) {
    const i = Number(b.dataset.index),
      c = r.comments[i];
    if (!c) return;
    openDialog(
      "Moderate this comment",
      `<p>Replace the visible comment with a tombstone. Its references and history remain available to permitted readers.</p><p class="small muted">${esc(c.by)} · ${esc(c.text)}</p>`,
      `<button class="btn" data-action="close-dialog">Cancel</button><button class="btn danger" data-action="cx-confirm-moderation" data-module="${r.module}" data-record="${r.id}" data-index="${i}">Confirm moderation</button>`,
    );
  }
  if (
    a === "cx-confirm-moderation" &&
    r &&
    ["owner", "manager"].includes(role().type)
  ) {
    const c = r.comments[Number(b.dataset.index)];
    if (c) {
      c.tombstone = true;
      wfHistory(r, "Moderated a comment; evidence references retained.");
      closeDialog();
      render();
    }
  }
  if (a === "cx-compose-tag" && $("post-text")) {
    $("post-text").value += " " + b.dataset.target;
    captureDraft();
    $("post-text").focus();
  }
  if (a === "wf-agent-reject" && r) {
    r.finished = true;
    r.status = "Rejected";
    wfHistory(r, "Rejected the sample agent output. Nothing was published.");
    render();
  }
});
document.addEventListener("input", (e) => {
  if (e.target.id === "cx-query") renderPicker(e.target.value);
  else if (e.target.dataset.enhanced && e.target.value.endsWith("@")) {
    const b = e.target.nextElementSibling?.querySelector(
      '[data-action="cx-picker"]',
    );
    if (b) openReferencePicker(b);
  }
});
document.addEventListener(
  "keydown",
  (e) => {
    const picker = $("reference-picker");
    if (!picker) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      const editor = activePicker?.editor;
      picker.remove();
      activePicker = null;
      $(editor)?.focus();
    }
    if (["ArrowDown", "ArrowUp"].includes(e.key)) {
      const choices = [...picker.querySelectorAll('[role="option"]')];
      if (!choices.length) return;
      e.preventDefault();
      const index = choices.indexOf(document.activeElement),
        next =
          e.key === "ArrowDown"
            ? (index + 1) % choices.length
            : (index - 1 + choices.length) % choices.length;
      choices[next].focus();
    }
  },
  true,
);

window.ERPX_CONNECTIONS = {
  composerTags,
  topicDirectory,
  topicView,
  journeyView,
  postDetailView,
  mentionItems,
  enhanceSurface,
  enrichDialog,
};

function formReferences(form) {
  return [...form.querySelectorAll("[data-enhanced]")].flatMap((el) =>
    editorRefsFor(el.id),
  );
}
function retainFieldReferences(r, form) {
  r.fieldRefs ||= {};
  for (const el of form.querySelectorAll("[data-enhanced]")) {
    const refs = editorRefsFor(el.id);
    r.fieldRefs[el.name] = refs;
    for (const ref of refs)
      if (
        !r.refs.some(
          (x) =>
            x.type === ref.type && x.module === ref.module && x.id === ref.id,
        ) &&
        r.refs.length < 50
      )
        r.refs.push(ref);
  }
}
window.ERPX_CONNECTIONS.formReferences = formReferences;
window.ERPX_CONNECTIONS.retainFieldReferences = retainFieldReferences;

window.ERPX_CONNECTIONS.referencePanel = referencePanel;
