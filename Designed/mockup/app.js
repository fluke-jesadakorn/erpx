/* ERPX workplace prototype. All authorization, data, and outcomes are local simulations. */
"use strict";
const SEED = window.ERPX_SEED;
const $ = (id) => document.getElementById(id);
const esc = (value) =>
  String(value ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const clone = (value) => JSON.parse(JSON.stringify(value));
const paths = {
  home: "M3 10l9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1z",
  briefcase: "M4 7h16v13H4z M8 7V4h8v3 M4 12h16 M10 12v3h4v-3",
  users:
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M17 4a4 4 0 0 1 0 7 M22 21v-2a4 4 0 0 0-3-3.9",
  book: "M3 4h6a4 4 0 0 1 3 2 4 4 0 0 1 3-2h6v15h-6a4 4 0 0 0-3 2 4 4 0 0 0-3-2H3z M12 6v15",
  inbox: "M4 4h16l2 11v5H2v-5z M2 15h6l2 3h4l2-3h6",
  search: "M21 21l-5-5 M18 10a8 8 0 1 0-16 0 8 8 0 0 0 16 0",
  chevron: "M9 5l7 7-7 7",
  down: "M6 9l6 6 6-6",
  plus: "M12 5v14 M5 12h14",
  close: "M6 6l12 12 M18 6L6 18",
  check: "M5 12l4 4L19 6",
  building:
    "M4 21V3h12v18 M16 9h4v12 M8 7h4 M8 11h4 M8 15h4 M9 21v-3h2v3 M2 21h20",
  shield: "M12 3l8 3v6c0 5-8 9-8 9s-8-4-8-9V6z M8 12l3 3 5-6",
  bell: "M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9 M10 21h4",
  sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1 1 M18 18l1 1 M5 19l1-1 M18 6l1-1",
  moon: "M20 15a8 8 0 0 1-11-11A9 9 0 1 0 20 15",
  menu: "M4 6h16 M4 12h16 M4 18h16",
  clock: "M12 8v5l3 2 M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0",
  star: "M12 3l3 6 7 1-5 5 1 7-6-3-6 3 1-7-5-5 7-1z",
  image: "M3 3h18v18H3z M3 16l5-5 5 5 3-3 5 5 M16 7h.01",
  video: "M3 5h12v14H3z M15 10l6-4v12l-6-4",
  play: "M8 4l13 8-13 8z",
  pause: "M7 4v16 M17 4v16",
  message: "M21 11a9 8 0 0 1-9 8H7l-5 3 2-6a8 8 0 0 1-1-5 9 8 0 0 1 18 0",
  heart:
    "M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8z",
  link: "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-2 2 M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l2-2",
  follow: "M12 3v12 M7 10l5 5 5-5 M5 18v3h14v-3",
  box: "M3 7l9-5 9 5v10l-9 5-9-5z M3 7l9 5 9-5 M12 12v10 M7 4l10 6",
  file: "M5 3h9l5 5v13H5z M14 3v6h5 M8 13h8 M8 17h5",
  cart: "M2 3h3l3 13h11l3-9H6 M9 21h.01 M18 21h.01",
  chart: "M4 3v18h17 M8 16V9 M13 16V5 M18 16v-5",
  settings: "M4 7h16 M4 17h16 M8 4v6 M16 14v6",
  arrow: "M5 12h14 M14 7l5 5-5 5",
  back: "M19 12H5 M10 7l-5 5 5 5",
  lock: "M5 10h14v11H5z M8 10V6a4 4 0 0 1 8 0v4 M12 14v3",
  flag: "M4 22V3 M4 3h15l-3 5 3 5H4",
  sparkle: "M12 3l2 7 7 2-7 2-2 7-2-7-7-2 7-2z",
  refresh:
    "M20 6v5h-5 M4 18v-5h5 M5 7a8 8 0 0 1 13-2l2 3 M4 16l2 3a8 8 0 0 0 13-2",
  globe:
    "M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0 M3 12h18 M12 3c-5 5-5 13 0 18 5-5 5-13 0-18",
  calendar: "M3 5h18v16H3z M3 10h18 M7 2v6 M17 2v6",
  server: "M3 3h18v7H3z M3 14h18v7H3z M7 6h.01 M7 17h.01",
  wallet: "M3 5h17v15H3z M16 10h6v6h-6z M3 5l14-3v3",
  award: "M12 3a6 6 0 1 0 0 12 6 6 0 0 0 0-12 M8 14l-2 8 6-3 6 3-2-8",
  mail: "M3 4h18v16H3z M3 5l9 8 9-8",
  alert: "M12 3l10 18H2z M12 9v5 M12 17h.01",
  dots: "M5 12h.01 M12 12h.01 M19 12h.01",
  trash: "M3 6h18 M5 6l1 15h12l1-15 M9 6V3h6v3 M10 10v7 M14 10v7",
};
// Render SVG synchronously so icons also work in dynamically opened dialogs.
const lucideNames = {
  home: "House", book: "BookOpen", chevron: "ChevronRight", down: "ChevronDown",
  close: "X", building: "Building2", image: "Image", message: "MessageCircle",
  follow: "UserPlus", box: "Package", file: "FileText", cart: "ShoppingCart",
  chart: "ChartNoAxesColumnIncreasing", settings: "SlidersHorizontal",
  arrow: "ArrowRight", back: "ArrowLeft", sparkle: "Sparkles", refresh: "RefreshCw",
  alert: "TriangleAlert", dots: "Ellipsis", trash: "Trash2", grid: "LayoutGrid",
  community: "Users", info: "Info",
};
const icon = (name, cls = "") => {
  const key = lucideNames[name] || name.charAt(0).toUpperCase() + name.slice(1);
  const node = window.lucide?.icons[key] || window.lucide?.icons.Package;
  if (node) return window.lucide.createElement(node, {
    class: cls, "stroke-width": 1.75, "aria-hidden": "true", focusable: "false",
    "data-icon": key,
  }).outerHTML;
  return `<svg class="${esc(cls)}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${paths[name] || paths.box}"/></svg>`;
};
const moduleIcons = {
  brand: "sparkle",
  commercial: "wallet",
  identity_access: "shield",
  organization: "building",
  platform_administration: "server",
  customer_360: "users",
  customer_authority: "lock",
  support: "message",
  finance: "wallet",
  sales: "file",
  pos: "cart",
  assets: "box",
  inventory: "box",
  maintenance: "settings",
  operations: "chart",
  procurement: "file",
  quality: "award",
  legal: "shield",
  people_payroll: "users",
  crm: "chart",
  marketing: "flag",
  manufacturing: "settings",
  planning: "calendar",
  projects: "briefcase",
  ecommerce: "cart",
  benefits: "heart",
  learning: "book",
  talent: "users",
  field_service: "briefcase",
  agent: "sparkle",
  collaboration: "message",
};
const NAV = [
  ["home", "Home", "home"],
  ["work", "Work", "briefcase"],
  ["learn", "Learn", "book"],
  ["inbox", "Inbox", "inbox"],
];
let state,
  records,
  posts,
  stories,
  shorts,
  draft,
  activeReview,
  pendingTimer,
  dialogReturn,
  version = 0;
const nowLabel = "Just now";
function defaultFields(k, title, subtitle = "") {
  const values = {},
    sample = !!title;
  const texts = {
    owner: "Mali Srisai",
    vendor: "ThaiMRO",
    from: "Receiving area",
    to: "Stockroom A",
    candidate: "Candidate A",
    register: "Register 2",
    asset: "FL-02",
    lot: "LOT-331",
    inspector: "Dao Arun",
    audience: "Customer subscribers",
    version: "v1",
    counterparty: "Siam Retail",
    dates: "September 12–14, 2026",
    member: "Mali Srisai",
    course: "Workplace essentials",
    site: "Bangkok branch",
    tech: "Field service team",
    model: "Demo writing assistant",
    participants: "Sales and Operations",
    principal: "Mali Srisai",
    role: "Sales associate",
    parent: "Acme Holding",
    job: "Workspace provisioning",
    target: title,
    window: "September 5, 02:00–03:00",
    subject: title,
  };
  const amount = (subtitle + " " + title).match(
    /\$([\d,]+(?:\.\d+)?)([kKmM])?/,
  );
  for (const f of SEED.FIELDS[k] || []) {
    values[f[0]] =
      f[1] === "choice"
        ? f[5][0]
        : f[1] === "bool"
          ? false
          : f[1] === "date"
            ? "2026-09-12"
            : f[1] === "int"
              ? f[0] === "progress"
                ? 40
                : 30
              : f[1] === "money"
                ? amount
                  ? Number(amount[1].replaceAll(",", "")) *
                    (amount[2]?.toLowerCase() === "k"
                      ? 1000
                      : amount[2]?.toLowerCase() === "m"
                        ? 1000000
                        : 1)
                  : 18400
                : sample
                  ? texts[f[0]] || title
                  : "";
  }
  return values;
}
function route(page = "home") {
  return state.holding === "platform"
    ? `#/platform${page === "home" ? "" : "/" + page}`
    : `#/holdings/${state.holding}/${page}`;
}
function recordRoute(r) {
  return route(`rec/${r.module}/${encodeURIComponent(r.id)}`);
}
function go(page = "home") {
  const target = route(page);
  if (location.hash === target) render();
  else location.hash = target;
}
function ctxKey() {
  return [
    state.persona,
    state.holding,
    state.role,
    state.branch,
    state.department,
  ].join(":");
}
function avatar(
  name = person().name,
  av = person().av,
  tone = person().tone,
  big = false,
) {
  return `<span class="avatar ${tone || "violet"} ${big ? "big" : ""}" aria-label="${esc(name)}">${esc(
    av ||
      name
        .split(" ")
        .map((n) => n[0])
        .slice(0, 2)
        .join(""),
  )}</span>`;
}
function symbol(k, cls = "") {
  return `<span class="module-symbol ${cls}">${icon(moduleIcons[k] || k)}</span>`;
}
function badge(status) {
  const green = [
    "Active",
    "Posted",
    "Approved",
    "Received",
    "Done",
    "Published",
    "Completed",
    "Enrolled",
    "Won",
    "Resolved",
    "Fulfilled",
    "Released",
    "Signed",
    "Accepted",
  ];
  const red = [
    "Blocked",
    "Rejected",
    "Failed",
    "Revoked",
    "Suspended",
    "Overdue",
    "Exception",
    "Expired",
  ];
  return `<span class="badge ${green.includes(status) ? "green" : red.includes(status) ? "red" : "amber"}"><span class="dot"></span>${esc(status)}</span>`;
}
function notify(message) {
  $("toast").textContent = message;
  $("toast").classList.add("visible");
  clearTimeout(notify.timer);
  notify.timer = setTimeout(() => $("toast").classList.remove("visible"), 3500);
}
function heading(title, subtitle, action = "") {
  return `<div class="page-heading"><div><h1>${title}</h1><p>${esc(subtitle)}</p></div>${action || `<span class="date">${icon("calendar")} Saturday, 5 September</span>`}</div>`;
}
function empty(title, desc, action = "", sym = "inbox") {
  return `<div class="card empty">${symbol(sym)}<h2>${esc(title)}</h2><p>${esc(desc)}</p>${action}</div>`;
}
function storageNotice() {
  if (state.storageConflict)
    return `<div class="notice amber storage-notice" role="alert">${icon("alert")}<span class="grow"><strong>Newer changes were saved in another tab.</strong><small>Load them, or keep this tab’s version.</small></span><button class="btn" data-storage-action="reload">Load newer</button><button class="btn primary" data-storage-action="overwrite">Keep this version</button></div>`;
  if (state.storageError)
    return `<div class="notice amber storage-notice" role="alert">${icon("alert")}<span>Browser storage is unavailable. Current changes may not survive a reload.</span></div>`;
  return "";
}
function shell(content, page) {
  const platform = state.holding === "platform";
  const active = ["ctx", "rec", "tasks", "modules", "journeys"].includes(page)
    ? "work"
    : page;
  const focused = ["tasks", "rec"].includes(page);
  const links = platform
    ? [
        ["home", "Overview", "chart"],
        ["work", "Platform modules", "server"],
        ["inbox", "Service inbox", "inbox"],
      ]
    : NAV;
  const nav = links
    .map(
      ([id, label, ico]) =>
        `<a class="nav-item ${active === id ? "active" : ""}" ${active === id ? 'aria-current="page"' : ""} href="${route(id)}">${icon(ico)}${label}${id === "inbox" ? `<span class="count">${notifications().filter((n) => !state.read.includes(actorKey(n.id))).length}</span>` : ""}</a>`,
    )
    .join("");
  $("app").innerHTML =
    `<div class="shell ${focused ? "task-focus" : ""}"><button class="mobile-overlay ${state.mobileOpen ? "open" : ""}" data-action="menu" aria-label="Close navigation"></button><aside class="sidebar ${state.mobileOpen ? "open" : ""}" aria-label="Main navigation"><a href="${route()}" class="brand"><span class="brand-mark">${icon("briefcase")}</span>erpx</a><nav>${nav}</nav><div class="favorites"><div class="between nav-label"><span class="eyebrow">FAVORITES</span><a href="${route("modules")}" aria-label="Manage favorite modules">${icon("plus")}</a></div>${state.favorites
      .filter(moduleAllowed)
      .map(
        (k) =>
          `<a href="${route("ctx/" + k)}" class="favorite-link">${icon(moduleIcons[k])}${esc(SEED.CTX[k].n)}</a>`,
      )
      .join(
        "",
      )}</div><div class="sidebar-bottom"><button class="demo-launch" data-action="demo">${icon("settings")}<span><strong>Explore the demo</strong>Simulated permissions · reset</span></button><div class="sidebar-person" title="Demo persona · ${esc(person().name)}">${avatar()}<span><strong>${esc(person().name)}</strong><small>${esc(role().name)}</small></span></div></div></aside><div class="main-shell"><header class="topbar"><button class="icon-btn mobile-menu" data-action="menu" aria-label="Open navigation">${icon("menu")}</button><button class="workspace-btn context-control" data-action="workspace" aria-label="Change customer, business unit or role"><span class="grow"><strong>${esc(platform ? "Platform Console" : holding().short + " · " + branchName())}</strong><small><span class="demo-indicator" title="Simulated permissions · saved locally">Demo</span> ${esc(role().name)}${!platform && state.department !== "all" ? " · " + esc(state.department) : ""}</small></span>${icon("down")}</button><button class="search-launch" data-action="search" aria-label="Search permitted records and posts">${icon("search")}<span>Search…</span><kbd>⌘ K</kbd></button><div class="top-actions"><button class="icon-btn theme-toggle" data-action="theme" aria-label="Toggle color theme">${icon(document.documentElement.classList.contains("dark") ? "sun" : "moon")}</button></div></header><main id="main" tabindex="-1">${storageNotice()}${state.demoState === "normal" ? "" : `<div class="notice demo-banner" role="status">${icon("settings")}<span class="grow">Demo scenario: <strong>${esc(state.demoState)}</strong></span><button class="link" data-action="clear-state">Clear</button></div>`}${content}</main></div></div>${focused ? "" : `<nav class="mobile-nav" aria-label="Mobile navigation">${links.map(([id, label, ico]) => `<a href="${route(id)}" class="${active === id ? "active" : ""}" ${active === id ? 'aria-current="page"' : ""}>${icon(ico)}<span>${label}</span></a>`).join("")}</nav>`}`;
  document.body.classList.toggle("focused-screen", focused);
  syncNavigation();
  document.title = `${platform ? "Platform" : SEED.CTX[parseRoute().key]?.n || links.find((n) => n[0] === active)?.[1] || "Work"} · ERPX`;
}
function syncNavigation() {
  const mobile = innerWidth <= 760;
  const sidebar = document.querySelector(".sidebar");
  if (sidebar) sidebar.inert = mobile && !state.mobileOpen;
  for (const el of document.querySelectorAll(".main-shell,.mobile-nav"))
    el.inert = mobile && state.mobileOpen;
}
window.addEventListener("resize", syncNavigation);
function parseRoute() {
  let hash = decodeURI(location.hash.replace(/^#\/?/, "").split("?")[0]);
  if (hash === "main") return { page: "home", holding: state.holding };
  if (!hash || hash === "feed" || hash === "today")
    return { page: "home", holding: state.holding };
  let parts = hash.split("/");
  if (parts[0] === "h")
    return {
      holding: parts[1],
      page: parts[2] || "home",
      key: parts[3],
      id: parts.slice(4).join("/"),
    };
  if (parts[0] === "platform")
    return {
      holding: "platform",
      page: parts[1] || "home",
      key: parts[2],
      id: parts.slice(3).join("/"),
    };
  if (parts[0] === "ctx" || parts[0] === "rec")
    return {
      holding: state.holding,
      page: parts[0],
      key: parts[1],
      id: parts.slice(2).join("/"),
    };
  return { holding: state.holding, page: parts[0] };
}
function render() {
  let p = parseRoute();
  if (p.page === "community") {
    // Preserve the link's workspace boundary and replace, rather than push, history.
    const canonical = location.hash.replace(/(^#\/?|\/)community(?=[?\/]|$)/, "$1home");
    history.replaceState(history.state, "", canonical);
    p = parseRoute();
  }
  const feedContext = ctxKey();
  if (state.homeFeedContext !== feedContext) {
    state.community = "all";
    state.feedTab = "all";
    state.homeFeedContext = feedContext;
  }
  if (!homeDepartments().includes(state.community)) state.community = "all";
  let content = "";
  if (p.holding !== state.holding) {
    content = empty(
      "This link is outside your workspace",
      "Choose a permitted workspace first. Content from another customer is never previewed here.",
      `<button class="btn primary" data-action="workspace">Choose workspace</button>`,
      "lock",
    );
  } else if (["denied", "expired"].includes(state.demoState)) {
    content = empty(
      "You do not have access here",
      "Your work context cannot open this information. Your other permitted work is still available.",
      `<button class="btn primary" data-action="clear-state">Return to my workspace</button>`,
      "lock",
    );
  } else if (state.demoState === "saving" && !["rec"].includes(p.page)) {
    content = `${heading("Getting your workspace ready", "Sample loading state")}<div class="card card-pad" role="status" aria-live="polite"><span class="muted small">Loading your work…</span><div class="skeleton wide"></div><div class="skeleton"></div><div class="skeleton"></div><button class="btn" data-action="clear-state">Finish loading</button></div>`;
  } else if (state.holding === "platform" && p.page === "home")
    content = platformView();
  else if (p.page === "home") content = homeView();
  else if (p.page === "work") content = taskWorkView();
  else if (p.page === "modules") content = workView();
  else if (p.page === "tasks") content = taskView(p.key, p.id);
  else if (p.page === "ctx") content = moduleView(p.key);
  else if (p.page === "rec") content = recordView(p.key, p.id);
  else if (p.page === "learn") content = learnView();
  else if (p.page === "inbox") content = inboxView();
  else if (p.page === "journeys") content = journeyView(p.key);
  else if (p.page === "tags") content = topicView(p.key);
  else if (p.page === "posts") content = postDetailView(p.key);
  else
    content = empty(
      "Let’s get you back to work",
      "That page is not part of this prototype.",
      `<a class="btn primary" href="${route()}">Go Home</a>`,
      "home",
    );
  shell(content, p.page);
  enhanceSurface();
  enhanceTasks();
  refreshAssistantDialog();
}
function actionable() {
  return taskItems();
}
function homeDepartments() {
  const permitted = assignment()?.departments || [];
  const departments = permitted.includes("all")
    ? [...new Set([...permitted, ...visiblePosts().map((p) => p.department)])]
    : permitted;
  return ["all", ...departments.filter((d) => d && d !== "all")];
}
function homeFeedControls() {
  return `<div class="home-updates-heading flex flex-wrap items-center justify-between gap-3"><div><h2 id="home-updates-title">Team updates</h2><p class="small muted">The conversations behind your work.</p></div><label class="feed-department"><span class="sr-only">Filter updates by department</span><select id="home-department">${homeDepartments().map((d) => `<option value="${esc(d)}" ${state.community === d ? "selected" : ""}>${d === "all" ? "All permitted departments" : esc(d)}</option>`).join("")}</select></label></div><div class="home-feed-tabs" aria-label="Filter team updates">${[["all", "All updates"], ["following", "Following"], ["team", "Team posts"], ["records", "Record updates"]].map(([id, label]) => `<button data-action="feed-tab" data-id="${id}" aria-pressed="${state.feedTab === id}" class="${state.feedTab === id ? "active" : ""}">${label}</button>`).join("")}</div>`;
}
function homeView() {
  const name = person().name.split(" ")[0];
  return `<div class="home-heading"><p class="eyebrow text-workspace-accent">Your workspace, connected</p><h1>Good morning, ${esc(name)}</h1><p class="home-subtitle text-workspace-muted">A clear next step. A connected team.</p></div><div class="home-layout"><div class="stack">${taskHomePanel()}${storyStrip()}<section class="home-updates stack" aria-labelledby="home-updates-title">${homeFeedControls()}${composerCard()}${feedView()}</section></div></div>`;
}
function storyStrip() {
  const ss = visibleStories().slice(0, 4);
  return `<section class="story-section" aria-label="Team Stories"><div class="between story-heading"><h2>Around your workplace</h2><span class="small muted">Stories · today</span></div><div class="stories"><button class="story-card create" data-action="new-story" aria-label="Create a Story"><img src="assets/celebrate.svg" alt="Illustrated team celebration"><span class="story-add">${icon("plus")}</span><span class="story-title">Your story</span></button>${ss.map((s) => `<button class="story-card" data-action="story" data-id="${s.id}" aria-label="View Story: ${esc(s.title)}"><img src="${s.image}" alt=""><span class="story-avatar">${avatar(s.by, s.av, "violet")}</span><span class="story-title">${esc(s.title)}<small>${esc(s.by.split(" ")[0])}</small></span></button>`).join("")}</div></section>`;
}
function composerCard() {
  return `<section class="card composer" aria-label="Create a post"><div class="composer-prompt">${avatar()}<button data-action="compose">Share an update, ${esc(person().name.split(" ")[0])}…${draft && draft.context === ctxKey() ? '<span class="draft-indicator">Draft kept</span>' : ""}</button></div></section>`;
}
function feedView() {
  let items = visiblePosts();
  if (state.feedTab === "team") items = items.filter((p) => p.kind === "post");
  if (state.feedTab === "records")
    items = items.filter((p) => p.kind === "record");
  const follows = (p) =>
    state.following.includes(actorKey(p.id)) ||
    isFollowed("person", p.by) ||
    (p.ref && isFollowed("record", p.ref.module + "/" + p.ref.id)) ||
    (p.tags || []).some((t) => isFollowed("tag", t));
  if (state.feedTab === "following") items = items.filter(follows);
  if (state.community !== "all" && parseRoute().page === "home")
    items = items.filter(
      (p) => p.department === state.community || !p.department,
    );
  if (state.demoState === "empty") items = [];
  if (!items.length)
    return empty(
      "Follow the work that matters",
      "Share an update or follow a record, person or topic to make this space your own.",
      `<button class="btn primary" data-action="compose">${icon("plus")}Share an update</button>`,
    );
  if (state.feedTab !== "all" || state.community !== "all")
    return items.map(postCard).join("");
  const followed = items.filter((p) => p.kind !== "system" && follows(p)),
    topic = items.filter((p) => !follows(p) && p.kind !== "system"),
    system = items.filter((p) => p.kind === "system");
  return [
    ["Following", followed],
    ["Topics & team updates", topic],
    ["System", system],
  ]
    .map(
      ([label, posts]) =>
        `<div class="feed-section-label">${icon(label === "Following" ? "users" : label === "System" ? "bell" : "message")}<strong>${label}</strong>${label.startsWith("Topics") ? `<a href="${route("ctx/collaboration/topics")}">Explore topics</a>` : ""}</div>${posts.length ? posts.map(postCard).join("") : '<p class="small muted">No new updates in this section.</p>'}`,
    )
    .join("");
}
function postCard(p) {
  const liked = state.liked.includes(actorKey(p.id)),
    open = state.commentsOpen.includes(p.id),
    r = p.ref ? findRecord(p.ref.module, p.ref.id) : null;
  const comments = p.kind === "record" && r ? publicComments(r) : p.comments;
  const audience = p.branch
    ? branchName(p.branch)
    : p.department
      ? `${p.department} team`
      : "Everyone at " + holding().short;
  return `<article class="card post" id="post-${p.id}"><div class="post-head">${avatar(p.by, p.av, p.tone)}<div class="grow"><strong>${esc(p.by)}</strong><div class="post-meta">${esc(p.time)} · ${icon(p.department ? "lock" : p.branch ? "building" : "globe")}${esc(audience)}</div></div><span class="badge ${p.kind === "record" ? "" : "gray"}">${p.kind === "record" ? "Record update" : p.kind === "system" ? "Workspace broadcast" : "Team post"}</span></div><div class="post-body">${esc(p.text)}</div>${postDiscovery(p)}${p.image ? `<img class="post-image" src="${p.image}" alt="Illustrated workplace moment shared by ${esc(p.by)}">` : ""}${r ? `<a class="record-preview" href="${recordRoute(r)}">${symbol(r.module)}<span class="grow"><strong>${esc(r.title)}</strong><small>${esc(SEED.CTX[r.module].n)} · ${esc(r.id)}</small></span>${badge(r.status)}${icon("chevron")}</a>` : ""}<div class="post-stats"><span><span class="reaction-bubble">${icon("heart")}</span>${p.likes + (liked ? 1 : 0)} appreciations</span><button class="muted" data-action="comments" data-id="${p.id}">${comments.length} comment${comments.length === 1 ? "" : "s"}</button></div><div class="post-actions"><button class="${liked ? "on" : ""}" data-action="like" data-id="${p.id}" aria-pressed="${liked}">${icon("heart")}Appreciate</button><button data-action="comments" data-id="${p.id}" aria-expanded="${open}">${icon("message")}Comment</button></div>${open && p.kind !== "record" ? `<div class="comment-list">${comments.map((c) => `<div class="comment">${avatar(c.by, null, "green")}<div class="comment-content"><strong>${esc(c.by)}</strong>${esc(c.text)}</div></div>`).join("")}</div><form class="comment-form" data-form="comment" data-id="${p.id}"><input name="text" aria-label="Comment on ${esc(p.by)}’s post" placeholder="Add to the conversation…" maxlength="1000" required><button class="btn soft" type="submit" aria-label="Send comment">${icon("arrow")}</button></form>` : ""}</article>`;
}
function workView() {
  const allowed = role().modules;
  let body = "";
  for (let i = 0; i < SEED.GROUPS.length; i++) {
    const g = SEED.GROUPS[i];
    if (state.group !== "all" && state.group !== String(i)) continue;
    const keys = g[1].filter(
      (k) =>
        allowed.includes(k) &&
        `${SEED.CTX[k].n} ${k}`
          .toLowerCase()
          .includes(state.filter.toLowerCase()),
    );
    if (!keys.length) continue;
    body += `<h2 class="group-heading">${esc(g[0])} <span class="badge gray">${keys.length}</span></h2><div class="module-grid">${keys.map((k) => `<div class="card module-card"><button class="icon-btn favorite-toggle ${state.favorites.includes(k) ? "on" : ""}" data-action="favorite" data-id="${k}" aria-label="${state.favorites.includes(k) ? "Remove" : "Add"} ${esc(SEED.CTX[k].n)} ${state.favorites.includes(k) ? "from" : "to"} favorites" aria-pressed="${state.favorites.includes(k)}">${icon("star")}</button><a href="${route("ctx/" + k)}">${symbol(k)}<h3 style="margin-top:13px">${esc(SEED.CTX[k].n)}</h3><p>${moduleDescription(k)}</p></a><div class="between"><span class="small muted">${visibleRecords(k).length} records</span><a href="${route("ctx/" + k)}" class="link" aria-label="Open ${esc(SEED.CTX[k].n)}">Open ${icon("arrow")}</a></div></div>`).join("")}</div>`;
  }
  if (!body)
    body = empty(
      "No matching modules",
      "Try another search or business function.",
    );
  return `${heading("Modules", `${allowed.length} available in your work context`, `<a class="btn" href="${route("work")}">${icon("back")}My Work</a>`)}<form class="filter-row" data-form="module-filter"><input name="query" aria-label="Search modules" placeholder="Find a module…" value="${esc(state.filter)}"><select name="group" aria-label="Business function"><option value="all">All business functions</option>${SEED.GROUPS.map((g, i) => `<option value="${i}" ${state.group === String(i) ? "selected" : ""}>${esc(g[0])}</option>`).join("")}</select></form>${body}`;
}
function moduleDescription(k) {
  const descriptions = {
    sales: "Quotes, orders, and customer commitments.",
    inventory: "Stock movements, reservations, and availability.",
    procurement: "Requests, suppliers, and purchase receipts.",
    finance: "Journals, periods, and financial review.",
    pos: "Daily sales, registers, and refunds.",
    organization: "Companies, branches, and working teams.",
    identity_access: "People’s roles and scoped access.",
    commercial: "Subscriptions, plans, and billing overview.",
    platform_administration: "Provisioning, backups, and service operations.",
    people_payroll: "People, leave, expenses, and payroll.",
    crm: "Opportunities and customer relationships.",
    support: "Customer questions, issues, and resolution.",
    collaboration: "Shared discussions and connected decisions.",
    learning: "Courses, assignments, and progress.",
  };
  return (
    descriptions[k] ||
    `Manage ${SEED.CTX[k].n.toLowerCase()} records and follow their progress.`
  );
}
function organizationTree() {
  return `<section class="card card-pad" style="margin-bottom:20px"><div class="section-head"><h2>Your organization</h2><p class="small muted">Branches belong to companies. Departments can work across branches.</p></div><div class="org-tree"><div class="org-node">${symbol("organization")}<span><strong>${esc(holding().name)}</strong><small>Customer workspace · billing root</small></span></div>${holding()
    .companies.map(
      (c) =>
        `<div class="org-node child">${icon("building")}<span><strong>${esc(c.name)}</strong><small>Company</small></span></div>${holding()
          .branches.filter((b) => b.company === c.id)
          .map(
            (b) =>
              `<div class="org-node grandchild">${icon("box")}<span><strong>${esc(b.name)}</strong><small>Branch · Sales, Operations & Customer service</small></span></div>`,
          )
          .join("")}`,
    )
    .join(
      "",
    )}<div class="notice" style="margin:8px 0 0">${icon("users")}Finance and People are shared departments across this customer workspace.</div></div></section>`;
}
function fieldHTML(f, value, disabled = false) {
  const [key, kind, label, max, required, options] = f;
  const attrs = `name="${key}" ${disabled ? "disabled" : ""} ${required ? "required" : ""}`;
  let input = "";
  if (kind === "choice")
    input = `<select ${attrs}>${options.map((o) => `<option ${String(value) === o ? "selected" : ""}>${esc(o)}</option>`).join("")}</select>`;
  else if (kind === "bool")
    input = `<input type="checkbox" ${attrs} ${value ? "checked" : ""}>`;
  else if (kind === "text" && max > 256)
    input = `<textarea rows="3" maxlength="${max}" ${attrs}>${esc(value)}</textarea>`;
  else
    input = `<input type="${kind === "money" || kind === "int" ? "number" : kind === "date" ? "date" : "text"}" ${kind === "money" ? 'min="0" step="0.01"' : kind === "int" ? `min="0" step="1" ${max ? 'max="' + max + '"' : ""}` : kind === "text" && max ? 'maxlength="' + max + '"' : ""} value="${esc(value)}" ${attrs}>`;
  return `<label class="field ${kind === "text" && max > 256 ? "full" : ""}">${esc(label)}${required ? " *" : ""}${input}</label>`;
}
function legacyRecordView(k, id) {
  const r = findRecord(k, id);
  if (!r)
    return empty(
      "Record unavailable",
      "This record is outside the current work context.",
      `<a class="btn" href="${route("work")}">Back to work</a>`,
      "lock",
    );
  const f = SEED.FLOW[k] || { stages: [r.status], acts: {} },
    stages = f.stages.includes(r.status) ? f.stages : [...f.stages, r.status];
  const acts = recordReadOnly(r) ? [] : f.acts[r.status] || [],
    locked =
      recordReadOnly(r) ||
      ["Posted", "Closed", "Archived", "Revoked", "Paid"].includes(r.status),
    dirty = state.recordDrafts[ctxKey() + ":" + r.id];
  return `<div class="focused-task-layout"><header class="focused-task-header"><div class="between"><button class="link" data-action="task-back">${icon("back")}Back</button><details class="task-tools"><summary>Record tools ${icon("dots")}</summary><div>${followButton("record", r.module + "/" + r.id)}${!locked ? `<button class="btn" data-action="task-assistant" data-module="${k}" data-record="${esc(id)}">Draft assistant</button>` : ""}</div></details></div><h1>${esc(r.title)}</h1><p>${esc(SEED.CTX[k].n)} · ${esc(r.id)}</p><div class="task-identity-line">${badge(r.status)}<span>${esc(branchName(r.branch))}</span></div></header><div class="task-main stack">${state.demoState === "stale" ? `<div class="notice amber"><div><strong>This record has changed.</strong><p>Refresh before saving or making a decision. Your draft is kept.</p><button class="btn" data-action="refresh-record">Refresh record</button></div></div>` : ""}${acts.length ? `<section class="card card-pad"><h2>Next action</h2><div class="flex wrap">${acts.map((a, i) => `<button class="btn ${i === 0 ? "primary" : ""}" data-action="review" data-id="${i}" ${!canTransition(r, a) ? "disabled" : ""}>${esc(a[0])}</button>`).join("")}</div>${!acts.some((a) => canTransition(r, a)) ? '<p class="small muted">An assigned reviewer must complete this action.</p>' : ""}</section>` : ""}<details class="task-disclosure" data-task-detail="details" open><summary>${icon("file")}Details <span>Fields, progress & history</span></summary><div class="task-disclosure-body"><form data-form="record-save" id="record-form"><div class="field-grid">${(SEED.FIELDS[k] || []).map((f) => fieldHTML(f, dirty?.[f[0]] ?? r.fields[f[0]], locked)).join("") || `<p>${esc(r.subtitle)}</p>`}</div><div class="save-bar"><span id="record-status" class="live-status" role="status">${dirty ? "Draft kept in this work context." : locked ? "Read-only record." : "Changes stay in this demo."}</span>${locked ? "" : '<button type="submit" class="btn">Save changes</button>'}</div></form><h3>Progress</h3><div class="lifecycle">${stages.map((s, i) => `<div class="stage ${s === r.status ? "current" : i < stages.indexOf(r.status) ? "done" : ""}"><span>${i + 1}</span>${esc(s)}</div>`).join("")}</div>${referencePanel(r)}<h3>History</h3>${r.history.map((h) => `<div class="activity-item"><strong>${esc(h.by)}</strong><p>${esc(h.text)}</p><small>${esc(h.time)}</small></div>`).join("")}</div></details><details class="task-disclosure" data-task-detail="discussion"><summary>${icon("message")}Discussion <span>${publicComments(r).length} replies</span></summary><div class="task-disclosure-body">${publicComments(
    r,
  )
    .map(
      (c) =>
        `<div class="comment">${avatar(c.by)}<div class="comment-content"><strong>${esc(c.by)}</strong>${esc(c.text)}</div></div>`,
    )
    .join(
      "",
    )}${recordReadOnly(r) ? "" : '<form class="flex" data-form="record-comment"><input name="text" class="grow" placeholder="Ask a question or leave an update…" aria-label="Record comment" maxlength="1000" required><button class="btn soft" type="submit">Send</button></form>'}</div></details></div></div>`;
}
function learnView() {
  let list = shorts.filter(
    (s) => state.learnFilter === "all" || state.saved.includes(actorKey(s.id)),
  );
  return `${heading("A little learning. A better workday.", "Practical ideas from your colleagues. Watch, save, and put them to work.")}<div class="between wrap" style="margin-bottom:22px"><div class="tabs" role="group" aria-label="Learning filter"><button class="${state.learnFilter === "all" ? "active" : ""}" data-action="learn-filter" data-id="all">All Shorts</button><button class="${state.learnFilter === "saved" ? "active" : ""}" data-action="learn-filter" data-id="saved">Saved for later</button></div><span class="small muted">Short lessons · captions included</span></div>${list.length ? `<div class="learn-grid">${list.map((s) => `<article class="card learn-card"><button class="learn-thumb" style="width:100%;padding:0" data-action="short" data-id="${s.id}" aria-label="Play ${esc(s.title)}"><img src="assets/${s.image}" alt=""><span class="play"><span>${icon("play")}</span></span><span class="duration">${s.duration}</span><span class="badge caption-chip">${s.category}</span></button><div class="learn-info"><h3>${esc(s.title)}</h3><p>${esc(s.description)}</p><div class="between"><span class="tiny muted">${esc(s.author)}</span><button class="btn ${state.saved.includes(actorKey(s.id)) ? "soft" : ""}" data-action="save-short" data-id="${s.id}" aria-pressed="${state.saved.includes(actorKey(s.id))}">${icon(state.saved.includes(actorKey(s.id)) ? "check" : "plus")}${state.saved.includes(actorKey(s.id)) ? "Saved" : "Save"}</button></div></div></article>`).join("")}</div>` : empty("Make room for a useful idea", "Save a Short and you can find it here whenever you need it.")}`;
}
function notifications() {
  if (state.holding === "platform")
    return records
      .filter((r) => isVisible(r))
      .map((r) => ({
        id: "n-" + r.id,
        title: r.title,
        text: "Sample service update · " + r.status,
        record: r,
        read: false,
      }));
  return visiblePosts()
    .slice(0, 5)
    .map((p) => ({
      id: "n-" + p.id,
      title:
        p.kind === "record"
          ? "A record you work with was updated"
          : `${p.by} shared an update`,
      text: p.text.split("\n")[0].slice(0, 95),
      post: p.id,
    }))
    .concat(
      mentionItems().map((r) => ({
        id: "mention-" + r.id,
        title: "You were mentioned",
        text: r.title,
        record: r,
      })),
    )
    .concat(
      actionable()
        .slice(0, 2)
        .map((r) => ({
          id: "n-" + r.holding + r.id,
          title: r.title,
          text: taskReason(r),
          task: true,
          record: r,
        })),
    );
}
function inboxView() {
  const ns = state.demoState === "empty" ? [] : notifications();
  return `${heading("Your inbox", "The updates and conversations that deserve your attention.", `<button class="btn" data-action="read-all">${icon("check")}Mark all read</button>`)}${ns.length ? `<div class="task-list">${ns.map((n) => `<article class="card notif-row ${state.read.includes(actorKey(n.id)) ? "read" : ""}">${symbol(n.record?.module || "message")}<button class="grow" style="text-align:left" data-action="notification" data-id="${esc(n.id)}"><strong>${esc(n.title)}</strong><p>${esc(n.text)}</p></button>${state.read.includes(actorKey(n.id)) ? `<span class="tiny muted">Read</span>` : '<span class="unread-dot" aria-label="Unread"></span>'}</article>`).join("")}</div>` : empty("You’re all caught up", "Mentions, followed conversations, and work updates will appear here.")}`;
}
function platformView() {
  return `${heading("A good day starts with a healthy platform.", "Your customer workspaces and service operations, in one place.", `<span class="badge green">${icon("check")}Demo service healthy</span>`)}<div class="metrics">${[
    ["Customer workspaces", "2"],
    [
      "Active branches",
      Object.values(HOLDINGS).reduce((n, h) => n + h.branches.length, 0),
    ],
    ["Service jobs", visibleRecords("platform_administration").length],
    ["Open access sessions", "0"],
  ]
    .map(
      ([k, v]) =>
        `<div class="card metric"><small>${k}</small><strong>${v}</strong></div>`,
    )
    .join(
      "",
    )}</div><div class="notice">${icon("shield")}Platform administration shows service information. Customer business records require their own authorized workspace. Use Demo persona to explore a customer experience.</div><h2 class="section-head">Your customers</h2><div class="platform-grid">${Object.entries(
    HOLDINGS,
  )
    .map(
      ([id, h]) =>
        `<section class="card customer-card"><div class="between"><div class="flex">${symbol("organization")}<div><h2>${esc(h.name)}</h2><span class="tiny muted">${esc(h.industry)}</span></div></div>${badge("Active")}</div><p>${h.companies.length} ${h.companies.length === 1 ? "company" : "companies"} · ${h.branches.length} branches</p><div class="line"></div><div class="between"><span class="small muted">Enterprise · Sample subscription</span><button class="btn" data-action="customer-info" data-id="${id}">Service details ${icon("arrow")}</button></div></section>`,
    )
    .join(
      "",
    )}</div><div class="card card-pad" style="margin-top:24px"><div class="between section-head"><h2>Recent service activity</h2><a class="link" href="${route("ctx/platform_administration")}">View jobs ${icon("arrow")}</a></div>${visibleRecords(
    "platform_administration",
  )
    .map(
      (r) =>
        `<a class="priority-item" href="${recordRoute(r)}"><span class="priority-icon">${icon("server")}</span><span class="grow"><strong>${esc(r.title)}</strong><p>${esc(r.subtitle)} · Sample event</p></span>${badge(r.status)}</a>`,
    )
    .join("")}</div>`;
}
function openDialog(title, body, footer = "", cls = "") {
  if ($("dialog").open) closeDialog();
  dialogReturn = document.activeElement;
  $("dialog").className = cls;
  $("dialog").innerHTML =
    `<div class="dialog-header"><h2 id="dialog-title">${esc(title)}</h2><button class="icon-btn" data-action="close-dialog" aria-label="Close dialog">${icon("close")}</button></div><div class="dialog-body">${body}</div>${footer ? `<div class="dialog-footer">${footer}</div>` : ""}`;
  $("dialog").showModal();
  enrichDialog();
  setTimeout(() => {
    const f = $("dialog").querySelector("[autofocus]");
    if (f) f.focus();
  }, 0);
}
function captureDraft() {
  if ($("dialog").open && $("post-text") && draft) {
    draft.text = $("post-text").value;
    draft.audience = $("post-audience").value;
    draft.ref = $("post-ref")?.value || "";
    state.postDrafts[draft.context + ":" + draft.mode] = clone(draft);
  }
}
function closeDialog() {
  captureDraft();
  clearTimeout(pendingTimer);
  version++;
  $("dialog")
    .querySelectorAll("video")
    .forEach((v) => v.pause());
  $("dialog").close();
  $("dialog").className = "";
  if (dialogReturn?.isConnected) dialogReturn.focus();
  else {
    const action = dialogReturn?.dataset?.action;
    const replacement = action
      ? document.querySelector('.topbar [data-action="' + action + '"]')
      : null;
    (replacement || $("main"))?.focus({ preventScroll: true });
  }
}
function openPersona() {
  openDialog(
    "Preview a different person",
    `<div class="notice">${icon("users")}Demo personas are fictional. This changes the person you are previewing; assigned role switching keeps their identity.</div>${Object.entries(
      personas,
    )
      .map(
        ([id, p]) =>
          `<button class="choice ${state.persona === id ? "active" : ""}" data-action="choose-persona" data-id="${id}">${avatar(p.name, p.av, p.tone)}<span class="grow"><strong>${esc(p.name)}</strong><small>${esc(p.description)}</small></span>${state.persona === id ? icon("check") : icon("chevron")}</button>`,
      )
      .join("")}`,
  );
}
function openRole() {
  const assignments = person().assignments.filter(
    (a) => a.holding === state.holding,
  );
  openDialog(
    "Switch your work role",
    `<div class="flex" style="margin-bottom:18px">${avatar()}<span><strong>${esc(person().name)}</strong><small class="muted" style="display:block">You stay signed in as the same person.</small></span></div>${assignments.map((a) => `<button class="choice ${a.role === state.role ? "active" : ""}" data-action="choose-role" data-id="${a.role}">${symbol("shield")}<span class="grow"><strong>${esc(roles[a.role].name)}</strong><small>${a.branches.includes("all") ? "All branches" : a.branches.map((b) => branchName(b)).join(", ")} · ${roles[a.role].modules.length} modules</small></span>${a.role === state.role ? icon("check") : icon("chevron")}</button>`).join("")}<p class="small muted">Only simulated assignments for this person are shown. Other personas are available in Demo controls.</p>`,
  );
}
function openWorkspace() {
  if (state.holding === "platform") {
    openDialog(
      "Platform Console",
      `<div class="notice">${icon("server")}You are in the platform administration layer.</div><p class="small muted">Customer access is represented separately from your platform role. Preview a customer persona to explore its workspace.</p>`,
      `<button class="btn" data-action="role">Switch assigned work role</button><button class="btn" data-action="demo">Explore customer personas</button>`,
    );
    return;
  }
  const available = [
    ...new Set(
      person()
        .assignments.filter((a) => a.holding !== "platform")
        .map((a) => a.holding),
    ),
  ];
  openDialog(
    "Your work context",
    `<form id="workspace-form" data-form="workspace"><label class="field">Customer workspace<select id="workspace-holding" name="holding">${available.map((id) => `<option value="${id}" ${id === state.holding ? "selected" : ""}>${esc(HOLDINGS[id].name)}</option>`).join("")}</select></label><div id="workspace-options"></div><div class="notice">${icon("shield")}Feeds, records, and search follow this context. Your work roles never grant access to another customer.</div><button class="btn primary" type="submit">Open workspace ${icon("arrow")}</button></form><section class="context-role-section"><h3>Assigned role</h3><p>${esc(person().name)} · ${esc(role().name)}</p><button class="btn" data-action="role">Switch assigned work role</button></section>`,
  );
  workspaceOptions(state.holding);
}
function workspaceOptions(hid, company) {
  const h = HOLDINGS[hid],
    a =
      person().assignments.find(
        (a) => a.holding === hid && a.role === state.role,
      ) || person().assignments.find((a) => a.holding === hid);
  const allowed = h.branches.filter(
    (b) => a.branches.includes("all") || a.branches.includes(b.id),
  );
  const companies = h.companies.filter((c) =>
    allowed.some((b) => b.company === c.id),
  );
  const currentCompany = company || "all";
  const bs = allowed.filter(
    (b) => currentCompany === "all" || b.company === currentCompany,
  );
  $("workspace-options").innerHTML =
    `${companies.length > 1 ? `<label class="field">Company<select name="company" id="workspace-company"><option value="all">All companies</option>${companies.map((c) => `<option value="${c.id}" ${currentCompany === c.id ? "selected" : ""}>${esc(c.name)}</option>`).join("")}</select></label>` : `<p class="small muted" style="margin-bottom:15px">${esc(companies[0].name)}</p>`}<label class="field">Branch<select name="branch">${a.branches.includes("all") && currentCompany === "all" ? '<option value="all">All branches</option>' : ""}${bs.map((b) => `<option value="${b.id}" ${hid === state.holding && b.id === state.branch ? "selected" : ""}>${esc(b.name)}</option>`).join("")}</select></label><label class="field">Department<select name="department">${a.departments.map((d) => `<option value="${d}" ${d === state.department ? "selected" : ""}>${d === "all" ? "All departments" : d}</option>`).join("")}</select></label>`;
}
function resetView() {
  state.filter = "";
  state.statusFilter = "all";
  state.feedTab = "all";
  state.community = "all";
  state.demoState = "normal";
  state.mobileOpen = false;
  state.recordDrafts = state.recordDrafts || {};
}
function setPersona(id) {
  if (!personas[id]) return;
  closeDialog();
  state.persona = id;
  const a = person().assignments[0];
  state.holding = a.holding;
  state.role = a.role;
  state.branch = a.branches[0];
  state.department = a.departments[0];
  resetView();
  go();
  notify(`Now previewing ${person().name}. Sample permissions applied.`);
}
function setRole(id) {
  const a = person().assignments.find(
    (a) => a.holding === state.holding && a.role === id,
  );
  if (!a) return;
  closeDialog();
  state.role = id;
  if (!a.branches.includes(state.branch)) state.branch = a.branches[0];
  if (!a.departments.includes(state.department))
    state.department = a.departments[0];
  resetView();
  go();
  notify(`${person().name} · ${role().name}`);
}
function openDemo() {
  openDialog(
    "Explore the demo",
    `<div class="notice">${icon("settings")}All people, permissions, service metrics, and transactions are simulated. Reloading resets business data.</div><button class="choice" data-action="persona">${avatar()}<span class="grow"><strong>Demo persona: ${esc(person().name)}</strong><small>Explore staff, manager, customer owner, or platform owner</small></span>${icon("chevron")}</button><button class="choice" data-action="role">${symbol("shield")}<span class="grow"><strong>Assigned work role</strong><small>${esc(role().name)} · preserves identity</small></span>${icon("chevron")}</button><div class="line"></div><h3>Explore an interface state</h3><p class="small muted" style="margin-top:5px">Choose a scenario, then interact with the workspace.</p><div class="demo-state-grid">${[
      ["normal", "Normal"],
      ["draft", "Draft"],
      ["saving", "Loading / saving"],
      ["success", "Success"],
      ["failure", "Save failure"],
      ["stale", "Stale record"],
      ["denied", "Access denied"],
      ["empty", "Empty view"],
    ]
      .map(
        ([id, label]) =>
          `<button class="btn ${state.demoState === id ? "active" : ""}" data-action="scenario" data-id="${id}">${label}</button>`,
      )
      .join(
        "",
      )}</div><div class="line"></div><button class="btn" data-action="theme">${icon("moon")}Toggle light / dark</button><p class="small muted" style="margin-top:15px">Stories leave the viewer after 24 hours. In this demo, expiration hides the Story while retaining its sample data for this session. Linked records remain available to permitted roles.</p>`,
    `<button class="btn danger" data-action="reset-confirm">${icon("refresh")}Reset demo</button>`,
  );
}
function allowedAudience() {
  const list = [["holding", `Everyone at ${holding().short}`]];
  if (state.branch !== "all") list.push(["branch", branchName()]);
  if (state.department !== "all")
    list.push(["department", `${state.department} team`]);
  return list;
}
function openComposer(mode = "post") {
  if (state.holding === "platform") {
    notify("Choose a customer persona to share workplace updates.");
    return;
  }
  if ($("dialog").open) closeDialog();
  if (!draft || draft.context !== ctxKey() || draft.mode !== mode)
    draft = state.postDrafts[ctxKey() + ":" + mode] || {
      context: ctxKey(),
      text: mode === "kudos" ? "A little appreciation for…" : "",
      audience: state.branch === "all" ? "holding" : "branch",
      image: mode === "photo" ? "assets/warehouse.svg" : null,
      ref: "",
      mode,
    };
  const choices = visibleRecords().slice(0, 100);
  openDialog(
    mode === "story" ? "Share a Story" : "Share with your workplace",
    `<div class="composer-modes"><button class="btn" data-action="compose-photo">Photo</button><button class="btn" data-action="new-story">Story</button><button class="btn" data-action="compose-kudos">Appreciation</button></div><form id="post-form" data-form="post"><div class="flex" style="margin-bottom:16px">${avatar()}<span><strong>${esc(person().name)}</strong><small class="muted" style="display:block">${esc(role().name)}</small></span></div><label class="field">${mode === "story" ? "Your Story caption" : "Your update"}<textarea id="post-text" name="text" rows="5" placeholder="A useful update, a question, or a moment worth sharing…" maxlength="${mode === "story" ? 280 : 3000}" required autofocus>${esc(draft.text)}</textarea></label><div class="compose-tools"><label class="field grow">Audience<select id="post-audience" name="audience">${allowedAudience()
      .map(
        ([id, label]) =>
          `<option value="${id}" ${draft.audience === id ? "selected" : ""}>${esc(label)}</option>`,
      )
      .join(
        "",
      )}</select></label><button type="button" class="btn" data-action="attach-sample">${icon("image")}${draft.image ? "Change sample" : "Sample photo"}</button></div><label class="field">Link a permitted business record (optional)<select id="post-ref" name="ref"><option value="">No linked record</option>${choices.map((r) => `<option value="${esc(r.module + "/" + r.id)}" ${draft.ref === r.module + "/" + r.id ? "selected" : ""}>${esc(r.id)} · ${esc(r.title)}</option>`).join("")}</select></label><div id="media-preview">${draft.image ? `<div class="media-preview"><img src="${draft.image}" alt="Sample workplace illustration"><button type="button" class="icon-btn" data-action="remove-sample" aria-label="Remove sample image">${icon("close")}</button></div>` : ""}</div><p id="post-status" class="form-status" role="status">Draft · kept in this work context until reload</p><div class="line"></div><div class="between"><span class="tiny muted">${mode === "story" ? "Visible for 24 hours · sample image" : "Team post · sample media only"}</span><button type="submit" class="btn primary">${icon("arrow")}${mode === "story" ? "Share Story" : "Publish update"}</button></div></form>`,
  );
}
function openStory(id) {
  const ss = visibleStories();
  const i = ss.findIndex((s) => s.id === id);
  if (i < 0) return;
  const s = ss[i];
  openDialog(
    s.by,
    `<div class="story-progress">${ss.map((_, j) => `<span class="${j <= i ? "active" : ""}"></span>`).join("")}</div><div class="media-stage"><img src="${s.image}" alt="Illustrated ${esc(s.title)}"><div class="media-overlay"><h3>${esc(s.title)}</h3><p>${esc(s.text)}</p>${s.ref && findRecord(s.ref.module, s.ref.id) ? `<a class="btn" style="margin-top:12px" href="${recordRoute(findRecord(s.ref.module, s.ref.id))}">Open linked record</a>` : ""}</div></div><div class="media-controls"><button class="btn" data-action="story" data-id="${ss[(i - 1 + ss.length) % ss.length].id}" aria-label="Previous Story">${icon("back")}</button><span class="tiny">${i + 1} / ${ss.length} · available today</span><button class="btn" data-action="story" data-id="${ss[(i + 1) % ss.length].id}" aria-label="Next Story">${icon("arrow")}</button></div>`,
    "",
    "media-dialog",
  );
}
function openShort(id) {
  const s = shorts.find((s) => s.id === id);
  if (!s || state.holding === "platform") return;
  openDialog(
    s.title,
    `<video id="short-video" controls playsinline preload="metadata" poster="assets/${s.image}" aria-label="${esc(s.title)}"><source src="assets/${s.video}" type="video/mp4"><track kind="captions" src="assets/${s.track}" srclang="en" label="English" default>Your browser cannot play this sample video.</video><div class="caption-note">${esc(s.description)} Sample lesson · English captions available in playback controls.</div><div class="media-controls"><span class="tiny">${esc(s.author)}</span><button class="btn" data-action="save-short" data-id="${id}">${icon(state.saved.includes(actorKey(id)) ? "check" : "plus")}${state.saved.includes(actorKey(id)) ? "Saved" : "Save for later"}</button></div>`,
    "",
    "media-dialog",
  );
}
function openSearch() {
  openDialog(
    "Find something in your workspace",
    `<label class="field">Search records, modules, and team posts<input id="search-input" placeholder="Try stock, Sales, or a teammate’s update…" autofocus></label><p class="tiny muted">${esc(scopeLabel())} · only permitted results appear</p><div id="search-results" class="search-results"></div>`,
  );
  searchResults("");
}
function searchResults(query) {
  if (!$("search-results")) return;
  const q = query.toLowerCase().replace(/^@/, "");
  const mods = role().modules.filter((k) =>
    SEED.CTX[k].n.toLowerCase().includes(q),
  );
  const rr = visibleRecords().filter((r) =>
    (r.module + "/" + r.id + " " + r.title).toLowerCase().includes(q),
  );
  const pp =
    state.holding === "platform"
      ? []
      : visiblePosts().filter((p) =>
          (p.text + " " + p.by).toLowerCase().includes(q),
        );
  const html = [
    ...taskItems()
      .filter((r) =>
        (taskAction(r)?.action + " " + r.title).toLowerCase().includes(q),
      )
      .map(
        (r) =>
          `<a class="search-result" href="${taskRoute(r)}">${icon("briefcase")}<span>${esc(taskAction(r)?.action || "Prepare next draft")}<small>${esc(r.title)} · Assigned task</small></span></a>`,
      ),
    ...mods.map(
      (k) =>
        `<a class="search-result" href="${route("ctx/" + k)}">${icon(moduleIcons[k])}<span>${esc(SEED.CTX[k].n)}<small>Module</small></span></a>`,
    ),
    ...rr.map(
      (r) =>
        `<a class="search-result" href="${recordRoute(r)}">${icon("file")}<span>${esc(r.title)}<small>${esc(r.id)} · ${esc(branchName(r.branch))}</small></span></a>`,
    ),
    ...pp.map(
      (p) =>
        `<button class="search-result" data-action="find-post" data-id="${p.id}" style="text-align:left">${icon("message")}<span>${esc(p.by)}<small>${esc(p.text.slice(0, 90))}</small></span></button>`,
    ),
  ].slice(0, 40);
  $("search-results").innerHTML =
    html.join("") ||
    '<p class="small muted">No permitted results match your search.</p>';
}
function formValues(form, k) {
  const values = {};
  for (const f of SEED.FIELDS[k] || []) {
    const el = form.elements[f[0]];
    if (!el) continue;
    values[f[0]] =
      f[1] === "bool"
        ? el.checked
        : f[1] === "money" || f[1] === "int"
          ? Number(el.value)
          : el.value.trim();
  }
  return values;
}
function openNewRecord(k) {
  if (!moduleAllowed(k) || moduleReadOnly(k)) return;
  openDialog(
    "New " + SEED.CTX[k].n.toLowerCase() + " record",
    `<form data-form="new-record" data-id="${k}"><label class="field">Record title *<input name="record_title" required maxlength="120" placeholder="A clear, useful name" autofocus></label><div class="field-grid">${(SEED.FIELDS[k] || []).map((f) => fieldHTML(f, defaultFields(k, "")[f[0]])).join("")}</div><p class="form-status" id="new-record-status" role="status">Created in ${esc(scopeLabel())} · sample data only</p><div class="line"></div><button type="submit" class="btn primary">Create draft record</button></form>`,
  );
}
function openReview(index) {
  const p = parseRoute(),
    r = findRecord(p.key, p.id);
  if (!r) return;
  const a = SEED.FLOW[r.module]?.acts[r.status]?.[index];
  if (!a || !canTransition(r, a)) return;
  activeReview = {
    holding: state.holding,
    module: r.module,
    id: r.id,
    version: r.version,
    from: r.status,
    action: a,
    context: ctxKey(),
  };
  openDialog(
    "Review " + a[0].toLowerCase(),
    `<div class="notice amber">${icon("shield")}This changes a business record. Reactions and comments cannot perform this action.</div><h3>${esc(r.title)}</h3><p class="small muted" style="margin:6px 0 18px">${esc(r.id)} · ${esc(branchName(r.branch))}</p><div class="between">${badge(r.status)}${icon("arrow")}${badge(a[1])}</div><div class="line"></div><p class="small">Acting as <strong>${esc(person().name)}</strong> · ${esc(role().name)}</p><label class="field" style="margin-top:15px">Decision note<input id="review-note" maxlength="300" placeholder="Add context for the next person (optional)"></label><label class="checkbox-label"><input id="review-confirm" type="checkbox">I have reviewed this sample record and its proposed change.</label><p id="review-status" class="form-status" role="status">No real transaction will be performed.</p>`,
    `<button class="btn" data-action="close-dialog">Cancel</button><button class="btn primary" data-action="confirm-review">Confirm ${esc(a[0])}</button>`,
  );
}
function runSave(statusEl, button, commit) {
  const token = ++version,
    context = ctxKey();
  button.disabled = true;
  statusEl.classList.remove("error");
  statusEl.textContent = "Saving…";
  clearTimeout(pendingTimer);
  pendingTimer = setTimeout(() => {
    if (token !== version || context !== ctxKey() || !statusEl.isConnected)
      return;
    button.disabled = false;
    if (state.demoState === "failure") {
      statusEl.classList.add("error");
      statusEl.textContent =
        "Could not save. Your draft is safe. Try again when ready.";
      state.demoState = "normal";
      return;
    }
    if (
      [
        "stale",
        "source-conflict",
        "offline",
        "degraded",
        "expired",
        "prerequisite",
      ].includes(state.demoState)
    ) {
      statusEl.classList.add("error");
      statusEl.textContent =
        "This action is blocked by the " +
        state.demoState +
        " scenario. Clear or refresh the scenario; your draft is kept.";
      return;
    }
    commit();
  }, 650);
}
function actorKey(id) {
  return state.holding + ":" + state.persona + ":" + id;
}
function toggle(list, value) {
  const i = list.indexOf(value);
  if (i < 0) list.push(value);
  else list.splice(i, 1);
}
function outcome(title, detail) {
  openDialog(
    "Action complete",
    `<div class="result"><div class="success-symbol">${icon("check")}</div><h3>${esc(title)}</h3><p>${esc(detail)}</p><span class="badge gray">Demo outcome · no real transaction</span></div>`,
    `<button class="btn primary" data-action="close-dialog">Back to work</button>`,
  );
}
function publishRecordUpdate(r, text) {
  posts.unshift({
    created: Date.now(),
    id: "update-" + Date.now(),
    holding: r.holding,
    branch: r.branch,
    department: null,
    module: r.module,
    kind: "record",
    tags: SEED.CTX[r.module]?.tags || [],
    by: person().name,
    av: person().av,
    tone: person().tone,
    text,
    time: nowLabel,
    likes: 0,
    comments: [],
    ref: { module: r.module, id: r.id },
  });
}
function showPost(id) {
  const p = visiblePosts().find((p) => p.id === id);
  if (!p) return;
  closeDialog();
  state.feedTab = "all";
  state.community = "all";
  if (!state.commentsOpen.includes(id)) state.commentsOpen.push(id);
  go("home");
  setTimeout(() => {
    const el = $("post-" + id);
    if (el && !$("dialog").open && parseRoute().page === "home") {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
      if (
        document.activeElement === $("main") ||
        document.activeElement === document.body
      ) {
        el.tabIndex = -1;
        el.focus({ preventScroll: true });
      }
    }
  }, 100);
}
document.addEventListener("click", (event) => {
  if (event.target.closest(".skip-link")) {
    event.preventDefault();
    $("main").focus();
    return;
  }
  const link = event.target.closest('a[href^="#/"]');
  if (link) {
    if ($("dialog").open) closeDialog();
    state.mobileOpen = false;
    state.filter = "";
    state.statusFilter = "all";
  }
  const button = event.target.closest("[data-action]");
  if (!button) return;
  const { action, id } = button.dataset;
  const p = parseRoute();
  const current = p.page === "rec" ? findRecord(p.key, p.id) : null;
  switch (action) {
    case "menu":
      state.mobileOpen = !state.mobileOpen;
      render();
      document
        .querySelector(state.mobileOpen ? ".sidebar .nav-item" : ".mobile-menu")
        ?.focus();
      break;
    case "theme":
      document.documentElement.classList.toggle("dark");
      savePrefs();
      render();
      break;
    case "workspace":
      openWorkspace();
      break;
    case "role":
      openRole();
      break;
    case "persona":
      openPersona();
      break;
    case "choose-persona":
      setPersona(id);
      break;
    case "choose-role":
      setRole(id);
      break;
    case "demo":
      openDemo();
      break;
    case "close-dialog":
      closeDialog();
      break;
    case "search":
      openSearch();
      break;
    case "tasks":
      state.workTab = "tasks";
      go("work");
      break;
    case "work":
      state.workTab = "modules";
      go("work");
      break;
    case "inbox":
      go("inbox");
      break;
    case "work-tab":
      state.workTab = id;
      go(id === "modules" ? "modules" : "work");
      break;
    case "favorite":
      if (!moduleAllowed(id)) break;
      toggle(state.favorites, id);
      savePrefs();
      render();
      break;
    case "feed-tab":
      state.feedTab = id;
      render();
      document.querySelector(`[data-action="feed-tab"][data-id="${state.feedTab}"]`)?.focus({ preventScroll: true });
      break;
    case "compose":
      openComposer();
      break;
    case "compose-photo":
      openComposer("photo");
      break;
    case "compose-kudos":
      openComposer("kudos");
      break;
    case "new-story":
      openComposer("story");
      break;
    case "attach-sample":
      captureDraft();
      draft.image =
        draft.image === "assets/warehouse.svg"
          ? "assets/team.svg"
          : "assets/warehouse.svg";
      openComposer(draft.mode);
      break;
    case "remove-sample":
      captureDraft();
      draft.image = null;
      openComposer(draft.mode);
      break;
    case "like":
      if (!visiblePosts().some((p) => p.id === id)) break;
      toggle(state.liked, actorKey(id));
      render();
      break;
    case "follow":
      if (!visiblePosts().some((p) => p.id === id)) break;
      toggle(state.following, actorKey(id));
      render();
      notify(
        state.following.includes(actorKey(id))
          ? "Following this conversation. Access stays the same."
          : "Conversation unfollowed.",
      );
      break;
    case "comments":
      {
        const post = visiblePosts().find((p) => p.id === id);
        const record =
          post?.kind === "record" &&
          post.ref &&
          findRecord(post.ref.module, post.ref.id);
        if (record) {
          location.hash = recordRoute(record) + "?view=discussion";
          break;
        }
      }
      if (!visiblePosts().some((p) => p.id === id)) break;
      toggle(state.commentsOpen, id);
      render();
      $("post-" + id)
        ?.querySelector("input")
        ?.focus({ preventScroll: true });
      break;
    case "story":
      openStory(id);
      break;
    case "short":
      openShort(id);
      break;
    case "save-short":
      if (!shorts.some((s) => s.id === id) || state.holding === "platform")
        break;
      toggle(state.saved, actorKey(id));
      if ($("dialog").open) {
        button.innerHTML =
          icon(state.saved.includes(actorKey(id)) ? "check" : "plus") +
          (state.saved.includes(actorKey(id)) ? "Saved" : "Save for later");
      } else render();
      notify(
        state.saved.includes(actorKey(id))
          ? "Saved to your learning list."
          : "Removed from saved learning.",
      );
      break;
    case "learn-filter":
      state.learnFilter = id;
      render();
      break;
    case "module-layout":
      state.moduleView = state.moduleView === "table" ? "board" : "table";
      render();
      break;
    case "new-record":
      openNewRecord(id);
      break;
    case "review":
      openReview(Number(id));
      break;
    case "confirm-review": {
      const review = activeReview,
        r = review && findRecord(review.module, review.id);
      if (
        !review ||
        !canTransition(r, review.action) ||
        review.context !== ctxKey()
      )
        break;
      const status = $("review-status");
      if (!$("review-confirm").checked) {
        status.textContent =
          "Please confirm that you have reviewed the proposed change.";
        status.classList.add("error");
        $("review-confirm").focus();
        break;
      }
      if (r.version !== review.version || r.status !== review.from) {
        status.textContent =
          "The record changed. Close this review and open it again.";
        status.classList.add("error");
        break;
      }
      const note = $("review-note").value.trim();
      runSave(status, button, () => {
        if (
          r.version !== review.version ||
          !canTransition(r, review.action) ||
          review.context !== ctxKey()
        ) {
          status.textContent = "Context changed. Reopen this review.";
          return;
        }
        r.status = review.action[1];
        r.version++;
        r.history.unshift({
          by: `${person().name} · ${role().name}`,
          text: `${review.action[0]} → ${r.status}${note ? ". " + note : ""}`,
          time: nowLabel,
        });
        publishRecordUpdate(r, `${review.action[0]} completed for ${r.id}.`);
        closeDialog();
        render();
        outcome(
          review.action[0] + " complete",
          `${r.id} is now ${r.status}. The sample record history includes your identity and role.`,
        );
      });
      break;
    }
    case "suggestion":
      if (!current) break;
      current.suggestion = id === "apply" ? "applied" : "rejected";
      render();
      if (id === "apply") {
        const field = document.querySelector(
          '[data-form="record-comment"] input',
        );
        field.value =
          "I’ve reviewed the current details. Please flag anything the next person should know.";
        field.focus();
        notify("Added to your comment draft. Review before sending.");
      }
      break;
    case "refresh-record":
      state.demoState = "normal";
      if (current) {
        current.version++;
        current.history.unshift({
          by: "Demo scenario",
          text: "Refreshed to the latest sample version. Review your draft before saving.",
          time: nowLabel,
        });
      }
      render();
      notify("Latest sample version loaded. Your draft is kept.");
      break;
    case "clear-state":
      state.demoState = "normal";
      render();
      break;
    case "scenario": {
      closeDialog();
      state.demoState = id;
      if (id === "draft") {
        state.demoState = "normal";
        if (state.holding === "platform") {
          openNewRecord("platform_administration");
        } else {
          openComposer();
          $("post-text").value =
            "Draft handover: morning delivery received. One item needs a second check.";
          captureDraft();
        }
      } else if (id === "success") {
        state.demoState = "normal";
        render();
        outcome(
          "Your change is saved",
          "This is the success state a user sees after a completed demo action.",
        );
      } else if (id === "stale") {
        const r =
          visibleRecords().find((r) => r.module === "sales") ||
          visibleRecords()[0];
        if (r) go(`rec/${r.module}/${encodeURIComponent(r.id)}`);
        else render();
      } else render();
      break;
    }
    case "reset-confirm":
      openDialog(
        "Reset this demo?",
        `<p class="small muted">Remove temporary posts, comments, record changes, saved Shorts, and role selections. Restore the original fictional data and Mali’s workspace. Your theme and favorite modules stay.</p>`,
        `<button class="btn" data-action="close-dialog">Keep exploring</button><button class="btn danger" data-action="reset">Reset demo</button>`,
      );
      break;
    case "reset":
      closeDialog();
      seed();
      go();
      render();
      notify("Demo restored. Theme and favorites kept.");
      break;
    case "read-all":
      state.read.push(
        ...notifications()
          .map((n) => n.id)
          .filter((id) => !state.read.includes(actorKey(id)))
          .map(actorKey),
      );
      render();
      notify("All updates marked as read.");
      break;
    case "notification": {
      const n = notifications().find((n) => n.id === id);
      if (!n) break;
      if (!state.read.includes(actorKey(id))) state.read.push(actorKey(id));
      if (n.record)
        go(
          `${n.task ? "tasks" : "rec"}/${n.record.module}/${encodeURIComponent(n.record.id)}`,
        );
      else showPost(n.post);
      break;
    }
    case "find-post":
      showPost(id);
      break;
    case "customer-info": {
      if (state.holding !== "platform" || !HOLDINGS[id]) break;
      const h = HOLDINGS[id];
      openDialog(
        h.name + " · service details",
        `<div class="notice">${icon("server")}Platform service metadata · simulated</div><div class="field-grid"><div class="field">Service state${badge("Active")}</div><div class="field">Subscription<strong>Enterprise · sample plan</strong></div><div class="field">Deployment<strong>Managed · sample environment</strong></div><div class="field">Last recovery check<strong>Passed · sample result</strong></div></div><div class="line"></div><h3>Customer access</h3><p class="small muted" style="margin-top:9px">No support session is active. Customer-approved support access is a separate process. This prototype uses fictional personas to preview the customer experience.</p>`,
        `<button class="btn primary" data-action="demo">Explore demo personas</button>`,
      );
      break;
    }
  }
});
document.addEventListener("submit", (event) => {
  const form = event.target;
  if (!form.dataset.form) return;
  event.preventDefault();
  const values = new FormData(form);
  switch (form.dataset.form) {
    case "module-filter":
      state.filter = String(values.get("query") || "");
      state.group = String(values.get("group") || "all");
      render();
      break;
    case "record-filter":
      state.filter = String(values.get("query") || "");
      state.statusFilter = String(values.get("status") || "all");
      render();
      break;
    case "workspace": {
      const hid = values.get("holding"),
        a =
          person().assignments.find(
            (a) => a.holding === hid && a.role === state.role,
          ) || person().assignments.find((a) => a.holding === hid);
      if (!a) break;
      const branch = values.get("branch"),
        department = values.get("department");
      if (!a.branches.includes(branch) && !a.branches.includes("all")) break;
      if (!a.departments.includes(department)) break;
      closeDialog();
      state.holding = hid;
      state.role = a.role;
      state.branch = branch;
      state.department = department;
      resetView();
      go();
      notify(`Workspace changed: ${holding().name} · ${scopeLabel()}`);
      break;
    }
    case "post": {
      captureDraft();
      if (!draft || draft.context !== ctxKey()) break;
      const text = draft.text.trim();
      if (!text) {
        $("post-status").textContent = "Write an update before publishing.";
        break;
      }
      if (!referencesVisible(editorRefsFor("post-text"))) {
        $("post-status").textContent =
          "A linked source is unavailable. Remove it or choose a permitted record.";
        return;
      }
      const audience = draft.audience;
      if (!allowedAudience().some(([id]) => id === audience)) break;
      let branch = audience === "branch" ? state.branch : null,
        department = audience === "department" ? state.department : null;
      const linked = draft.ref ? findRecord(...draft.ref.split("/")) : null;
      if (draft.ref && !linked) {
        $("post-status").textContent =
          "The linked record is no longer available.";
        break;
      }
      if (linked) branch = linked.branch;
      const payload = {
        id: "post-" + Date.now(),
        holding: state.holding,
        branch,
        department,
        module: linked?.module,
        kind: "post",
        by: person().name,
        av: person().av,
        tone: person().tone,
        text,
        created: Date.now(),
        refs: editorRefsFor("post-text"),
        tags: composerTags(),
        time: nowLabel,
        likes: 0,
        comments: [],
        image: draft.image,
        ref: linked ? { module: linked.module, id: linked.id } : null,
      };
      const story = draft.mode === "story";
      const image = draft.image || "assets/team.svg";
      runSave($("post-status"), form.querySelector('[type="submit"]'), () => {
        if (story)
          stories.unshift({
            ...payload,
            id: "story-" + Date.now(),
            title: text.slice(0, 42),
            text,
            image,
            created: Date.now(),
          });
        else posts.unshift(payload);
        delete state.editorRefs[editorKey("post-text")];
        const draftKey = draft.context + ":" + draft.mode;
        closeDialog();
        delete state.postDrafts[draftKey];
        draft = null;
        state.feedTab = "all";
        state.community = "all";
        go("home");
        render();
        notify(
          story
            ? "Story shared for today."
            : linked
              ? "Update published. Its audience also follows the linked record’s access."
              : "Update published to your chosen audience.",
        );
      });
      break;
    }
    case "comment": {
      const p = visiblePosts().find((p) => p.id === form.dataset.id);
      const text = String(values.get("text") || "").trim();
      if (p && text) {
        const target =
          p.kind === "record" && p.ref ? findRecord(p.ref.module, p.ref.id) : p;
        target.comments.push({
          by: person().name,
          text,
          refs: editorRefsFor(form.querySelector("input")?.id),
        });
        delete state.editorRefs[editorKey(form.querySelector("input")?.id)];
        render();
        notify("Comment added.");
      }
      break;
    }
    case "record-comment": {
      const p = parseRoute(),
        r = findRecord(p.key, p.id),
        text = String(values.get("text") || "").trim();
      if (r && text && !recordReadOnly(r)) {
        r.comments.push({
          by: person().name,
          text,
          refs: editorRefsFor(form.querySelector("input")?.id),
        });
        r.history.unshift({
          by: person().name,
          text: "Added a discussion comment.",
          time: nowLabel,
        });
        delete state.editorRefs[editorKey(form.querySelector("input")?.id)];
        render();
        notify("Comment added to this record.");
      }
      break;
    }
    case "record-save": {
      const p = parseRoute(),
        r = findRecord(p.key, p.id);
      if (
        !canEdit(r) ||
        ["Posted", "Closed", "Archived", "Revoked", "Paid"].includes(r.status)
      )
        break;
      if (recordReadOnly(r)) return;
      const data = formValues(form, r.module),
        key = ctxKey() + ":" + r.id;
      state.recordDrafts[key] = data;
      const expected = r.version;
      runSave($("record-status"), form.querySelector('[type="submit"]'), () => {
        if (!isVisible(r) || r.version !== expected) {
          $("record-status").textContent =
            "The record changed. Refresh and review your draft.";
          return;
        }
        if (!referencesVisible(formReferences(form))) {
          $("record-status").textContent =
            "A field reference is unavailable. Review its audience before saving.";
          return;
        }
        retainFieldReferences(r, form);
        r.fields = data;
        r.version++;
        r.history.unshift({
          by: `${person().name} · ${role().name}`,
          text: "Saved record details.",
          time: nowLabel,
        });
        delete state.recordDrafts[key];
        render();
        notify("Details saved in this demo.");
      });
      break;
    }
    case "new-record": {
      const k = form.dataset.id;
      if (!moduleAllowed(k) || moduleReadOnly(k)) break;
      const title = String(values.get("record_title") || "").trim();
      if (!title) break;
      const fields = formValues(form, k);
      runSave(
        $("new-record-status"),
        form.querySelector('[type="submit"]'),
        () => {
          const r = {
            holding: state.holding,
            branch:
              state.branch === "all"
                ? holding()?.branches[0]?.id || "all"
                : state.branch,
            department: state.department,
            module: k,
            id:
              k.slice(0, 4).toUpperCase() +
              "-" +
              Date.now().toString().slice(-7),
            title,
            subtitle: "New demo record",
            status: SEED.FLOW[k]?.stages[0] || "Open",
            version: 1,
            fields,
            comments: [],
            history: [
              {
                by: `${person().name} · ${role().name}`,
                text: "Created a draft record.",
                time: nowLabel,
              },
            ],
            suggestion: "ready",
          };
          records.unshift(r);
          if (state.holding !== "platform")
            publishRecordUpdate(r, "A new draft is ready: " + r.title);
          closeDialog();
          state.filter = "";
          state.statusFilter = "all";
          go(`rec/${k}/${r.id}`);
          notify("Draft record created in this workspace.");
        },
      );
      break;
    }
  }
});
document.addEventListener("input", (event) => {
  if (event.target.id === "search-input") searchResults(event.target.value);
  if (event.target.id === "post-text") {
    captureDraft();
    $("post-status").textContent =
      "Draft saved for this work context · not published";
  }
  if (event.target.closest("#record-form")) {
    const p = parseRoute(),
      r = findRecord(p.key, p.id);
    if (r) {
      state.recordDrafts[ctxKey() + ":" + r.id] = formValues(
        $("record-form"),
        r.module,
      );
      $("record-status").textContent = "Unsaved draft · kept while you explore";
    }
  }
});
document.addEventListener("change", (event) => {
  if (event.target.id === "home-department") {
    state.community = homeDepartments().includes(event.target.value) ? event.target.value : "all";
    render();
    $("home-department")?.focus({ preventScroll: true });
  }
  if (event.target.id === "workspace-holding")
    workspaceOptions(event.target.value);
  if (event.target.id === "workspace-company")
    workspaceOptions($("workspace-holding").value, event.target.value);
  if (event.target.id === "post-audience" || event.target.id === "post-ref")
    captureDraft();
});
document.addEventListener("keydown", (event) => {
  if (
    event.key === "Tab" &&
    state.mobileOpen &&
    innerWidth <= 760 &&
    !$("dialog").open
  ) {
    const focusable = [
      ...document.querySelectorAll(".sidebar a,.sidebar button"),
    ].filter((el) => el.getClientRects().length);
    const first = focusable[0],
      last = focusable.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    openSearch();
  }
  if (
    $("dialog").open &&
    $("dialog").classList.contains("media-dialog") &&
    !$("short-video") &&
    ["ArrowRight", "ArrowLeft"].includes(event.key)
  ) {
    const buttons = $("dialog").querySelectorAll('[data-action="story"]');
    buttons[event.key === "ArrowRight" ? 1 : 0]?.click();
  }
  if (event.key === "Escape" && state.mobileOpen && !$("dialog").open) {
    state.mobileOpen = false;
    render();
    document.querySelector(".mobile-menu")?.focus();
  }
});
$("dialog").addEventListener("cancel", (event) => {
  event.preventDefault();
  closeDialog();
});
$("dialog").addEventListener("click", (event) => {
  if (event.target === $("dialog")) {
    const rect = $("dialog").getBoundingClientRect();
    if (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    )
      closeDialog();
  }
});
window.addEventListener("hashchange", () => {
  if ($("dialog").open) closeDialog();
  state.mobileOpen = false;
  restoreNavigationView();
  render();
  $("main")?.focus({ preventScroll: true });
  restoreNavigationPosition();
});
window.ERPX_LEGACY = { legacyRecordView, organizationTree };
