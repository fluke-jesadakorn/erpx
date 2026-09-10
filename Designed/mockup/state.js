/* Workspace context, in-memory seed and shared visibility. */
const HOLDINGS = {
  acme: {
    name: "Acme Holding",
    short: "Acme",
    industry: "Retail & distribution",
    companies: [
      { id: "retail", name: "Acme Retail Co." },
      { id: "distribution", name: "Acme Distribution Co." },
    ],
    branches: [
      { id: "bkk", name: "Bangkok", company: "retail" },
      { id: "cnx", name: "Chiang Mai", company: "retail" },
      { id: "warehouse", name: "Central warehouse", company: "distribution" },
    ],
  },
  northstar: {
    name: "Northstar Supply",
    short: "Northstar",
    industry: "Wholesale supplies",
    companies: [{ id: "supply", name: "Northstar Supply Co." }],
    branches: [
      { id: "hq", name: "Head office", company: "supply" },
      { id: "east", name: "East depot", company: "supply" },
    ],
  },
};
const commonModules = [
  "collaboration",
  "learning",
  "benefits",
  "people_payroll",
];
const roles = {
  owner: {
    name: "Customer owner",
    type: "owner",
    modules: Object.keys(SEED.CTX).filter(
      (k) => k !== "platform_administration",
    ),
  },
  manager: {
    name: "Branch manager",
    type: "manager",
    modules: [
      "sales",
      "pos",
      "ecommerce",
      "crm",
      "customer_360",
      "support",
      "inventory",
      "procurement",
      "operations",
      "assets",
      "maintenance",
      "quality",
      "planning",
      "projects",
      "field_service",
      ...commonModules,
    ],
  },
  sales: {
    name: "Sales associate",
    type: "staff",
    modules: [
      "sales",
      "pos",
      "crm",
      "customer_360",
      "inventory",
      "support",
      ...commonModules,
    ],
  },
  warehouse: {
    name: "Warehouse specialist",
    type: "staff",
    modules: [
      "inventory",
      "procurement",
      "operations",
      "assets",
      "quality",
      "support",
      ...commonModules,
    ],
  },
  receiving: {
    name: "Receiving lead",
    type: "manager",
    modules: [
      "inventory",
      "procurement",
      "operations",
      "assets",
      "quality",
      "support",
      "maintenance",
      ...commonModules,
    ],
  },
  finance: {
    name: "Finance controller",
    type: "manager",
    modules: [
      "finance",
      "commercial",
      "procurement",
      "legal",
      "customer_360",
      ...commonModules,
    ],
  },
  platform: {
    name: "Platform owner",
    type: "platform",
    modules: ["platform_administration", "commercial"],
  },
  operator: {
    name: "Support operator",
    type: "platform",
    modules: ["platform_administration"],
  },
};
const personas = {
  mali: {
    name: "Mali Srisai",
    av: "MS",
    tone: "violet",
    description: "Branch manager · also works in Sales",
    assignments: [
      {
        holding: "acme",
        role: "manager",
        branches: ["bkk", "cnx"],
        departments: ["Sales", "Operations", "Customer service"],
      },
      {
        holding: "acme",
        role: "sales",
        branches: ["bkk"],
        departments: ["Sales"],
      },
    ],
  },
  dao: {
    name: "Dao Arun",
    av: "DA",
    tone: "green",
    description: "Warehouse specialist · receiving lead",
    assignments: [
      {
        holding: "acme",
        role: "warehouse",
        branches: ["warehouse"],
        departments: ["Operations"],
      },
      {
        holding: "acme",
        role: "receiving",
        branches: ["warehouse"],
        departments: ["Operations"],
      },
    ],
  },
  nisa: {
    name: "Nisa Chen",
    av: "NC",
    tone: "blue",
    description: "Customer owner · two customer workspaces",
    assignments: [
      {
        holding: "acme",
        role: "owner",
        branches: ["all", "bkk", "cnx", "warehouse"],
        departments: [
          "all",
          "Sales",
          "Operations",
          "Finance",
          "People",
          "Customer service",
        ],
      },
      {
        holding: "acme",
        role: "finance",
        branches: ["all", "bkk", "cnx", "warehouse"],
        departments: ["Finance"],
      },
      {
        holding: "northstar",
        role: "owner",
        branches: ["all", "hq", "east"],
        departments: ["all", "Sales", "Operations", "Finance", "People"],
      },
    ],
  },
  arun: {
    name: "Arun Patel",
    av: "AP",
    tone: "orange",
    description: "Northstar branch manager",
    assignments: [
      {
        holding: "northstar",
        role: "manager",
        branches: ["hq", "east"],
        departments: ["Sales", "Operations", "Customer service"],
      },
    ],
  },
  fluke: {
    name: "Fluke",
    av: "FL",
    tone: "violet",
    description: "Platform owner · customer service operations",
    assignments: [
      {
        holding: "platform",
        role: "platform",
        branches: ["all"],
        departments: ["all"],
      },
      {
        holding: "platform",
        role: "operator",
        branches: ["all"],
        departments: ["all"],
      },
    ],
  },
};
function prefs() {
  try {
    return JSON.parse(localStorage.getItem("erpx-workplace-prefs") || "{}");
  } catch {
    return {};
  }
}
function savePrefs() {
  try {
    localStorage.setItem(
      "erpx-workplace-prefs",
      JSON.stringify({
        theme: document.documentElement.classList.contains("dark")
          ? "dark"
          : "light",
        favorites: state.favorites,
      }),
    );
  } catch {}
}
function person() {
  return personas[state.persona];
}
function role() {
  return roles[state.role];
}
function holding() {
  return HOLDINGS[state.holding];
}
function assignment() {
  return person().assignments.find(
    (a) => a.holding === state.holding && a.role === state.role,
  );
}
function branchName(id = state.branch) {
  return id === "all"
    ? "All branches"
    : holding()?.branches.find((b) => b.id === id)?.name || "Platform";
}
function scopeLabel() {
  return state.holding === "platform"
    ? "Platform operations"
    : `${branchName()}${state.department === "all" ? "" : ` · ${state.department}`}`;
}
function initialState() {
  return {
    persona: "mali",
    holding: "acme",
    role: "manager",
    branch: "bkk",
    department: "Sales",
    feedTab: "all",
    workTab: "modules",
    moduleView: "table",
    group: "all",
    filter: "",
    statusFilter: "all",
    demoState: "normal",
    favorites: prefs().favorites || ["sales", "inventory", "procurement"],
    following: [],
    liked: [],
    saved: [],
    read: [],
    commentsOpen: [],
    recordDrafts: {},
    postDrafts: {},
    community: "all",
    learnFilter: "all",
    mobileOpen: false,
    capabilityRuns: {},
    attachmentDrafts: {},
    taskCommentDrafts: {},
    taskDetails: {},
  };
}
function seed() {
  state = initialState();
  records = [];
  posts = [];
  stories = [];
  draft = null;
  activeReview = null;
  for (const [hid, h] of Object.entries(HOLDINGS)) {
    for (const [key, c] of Object.entries(SEED.CTX)) {
      if (key === "platform_administration") continue;
      c.rows.forEach((row, i) => {
        const id = (hid === "northstar" ? "NS-" : "") + row[0];
        const branch = h.branches[i % h.branches.length].id;
        const department = ["finance", "legal", "commercial"].includes(key)
          ? "Finance"
          : ["people_payroll", "talent", "benefits", "learning"].includes(key)
            ? "People"
            : [
                  "sales",
                  "pos",
                  "crm",
                  "marketing",
                  "ecommerce",
                  "customer_360",
                ].includes(key)
              ? "Sales"
              : "Operations";
        const title =
          hid === "northstar"
            ? `Northstar · ${row[1].replace(/Acme Holding/g, "Harbor Stores")}`
            : row[1];
        const f = SEED.FLOW[key];
        const status = f?.alias[row[3]] || row[3];
        records.push({
          holding: hid,
          branch,
          department,
          module: key,
          id,
          title,
          subtitle: row[2],
          status,
          version: 1,
          fields: defaultFields(key, title, row[2]),
          comments: [],
          history: [
            {
              by: "Demo seed",
              text: `Sample record prepared in ${status}.`,
              time: "Earlier today",
            },
          ],
          suggestion: "ready",
        });
      });
    }
    const b = h.branches[0].id;
    posts.push({
      id: `${hid}-welcome`,
      holding: hid,
      branch: null,
      department: null,
      kind: "post",
      by: hid === "acme" ? "Nisa Chen" : "Arun Patel",
      av: hid === "acme" ? "NC" : "AP",
      tone: "blue",
      text:
        hid === "acme"
          ? "A little progress, a lot of teamwork. ✨\nOur Bangkok team made room for the new season today. Thank you for the care you put into the details — from the first delivery to the last customer."
          : "A fresh start at Northstar. Our East depot is ready for the new delivery schedule. Share your handover notes here so the next team has everything they need.",
      time: "25 min ago",
      likes: 18,
      comments: [
        {
          by: hid === "acme" ? "Dao Arun" : "Arun Patel",
          text: "Small improvements make the whole day smoother. Thanks, team!",
        },
      ],
      image: "assets/team.svg",
    });
    posts.push({
      id: `${hid}-delivery`,
      holding: hid,
      branch: b,
      department: null,
      kind: "post",
      by: hid === "acme" ? "Dao Arun" : "Arun Patel",
      av: hid === "acme" ? "DA" : "AP",
      tone: "green",
      text: "The morning delivery is checked and ready. Two cartons need a closer look — I have linked the stock record so we can keep the discussion in one place.",
      time: "48 min ago",
      likes: 7,
      comments: [],
      ref: {
        module: "inventory",
        id: (hid === "northstar" ? "NS-" : "") + "MOVE-883",
      },
    });
    posts.push({
      id: `${hid}-quote`,
      holding: hid,
      branch: b,
      department: null,
      kind: "record",
      by: "Sales team",
      av: "ST",
      tone: "violet",
      text: "Quote sent. The next step is ready when the customer is.",
      time: "1 hour ago",
      likes: 4,
      comments: [],
      ref: {
        module: "sales",
        id: (hid === "northstar" ? "NS-" : "") + "QT-2026-042",
      },
    });
    posts.push({
      id: `${hid}-restricted`,
      holding: hid,
      branch: null,
      department: "Finance",
      module: "finance",
      kind: "post",
      by: "Nisa Chen",
      av: "NC",
      tone: "blue",
      text: `${h.short} confidential margin review — Finance discussion only.`,
      time: "2 hours ago",
      likes: 2,
      comments: [],
    });
    posts.push({
      id: `${hid}-branch-note`,
      holding: hid,
      branch: h.branches[1].id,
      department: "Operations",
      kind: "post",
      by: "Operations team",
      av: "OT",
      tone: "orange",
      text: `${h.branches[1].name} handover: the afternoon receiving area is ready.`,
      time: "2 hours ago",
      likes: 3,
      comments: [],
    });
    const ss = [
      [
        "team",
        "A good start",
        "A little teamwork goes a long way. Here’s to a great shift.",
        "Nisa Chen",
        "NC",
        "team.svg",
      ],
      [
        "store",
        "On the floor",
        "Fresh shelves, familiar faces. Ready for the weekend.",
        "Mali Srisai",
        "MS",
        "store.svg",
      ],
      [
        "warehouse",
        "Behind the scenes",
        "The morning receiving team has everything in its place.",
        "Dao Arun",
        "DA",
        "warehouse.svg",
      ],
      [
        "celebrate",
        "Team moments",
        "Celebrating the people who make every day better.",
        "People team",
        "PT",
        "celebrate.svg",
      ],
    ];
    ss.forEach((s, i) =>
      stories.push({
        id: `${hid}-story-${i}`,
        holding: hid,
        branch: i === 2 ? h.branches[h.branches.length - 1].id : null,
        department: null,
        title: s[1],
        text: s[2],
        by: hid === "acme" ? s[3] : "Northstar team",
        av: s[4],
        image: `assets/${s[5]}`,
        created: Date.now() - i * 3600000,
      }),
    );
  }
  for (const row of SEED.CTX.platform_administration.rows)
    records.push({
      holding: "platform",
      branch: "all",
      department: "all",
      module: "platform_administration",
      id: row[0],
      title: row[1],
      subtitle: row[2],
      status: row[3],
      version: 1,
      fields: defaultFields("platform_administration", row[1]),
      comments: [],
      history: [
        {
          by: "Demo seed",
          text: "Sample platform job prepared.",
          time: "Earlier today",
        },
      ],
      suggestion: "ready",
    });
  for (const row of SEED.CTX.commercial.rows)
    records.push({
      holding: "platform",
      branch: "all",
      department: "all",
      module: "commercial",
      id: "PLATFORM-" + row[0],
      title: row[1],
      subtitle: row[2],
      status: SEED.FLOW.commercial.alias[row[3]] || row[3],
      version: 1,
      fields: defaultFields("commercial", row[1], row[2]),
      comments: [],
      history: [
        {
          by: "Demo seed",
          text: "Sample platform billing item prepared.",
          time: "Earlier today",
        },
      ],
      suggestion: "ready",
    });
  shorts = [
    {
      id: "receive",
      title: "A better way to receive stock",
      description: "Three small checks before you sign.",
      duration: "12 sec",
      image: "warehouse.svg",
      video: "receiving.mp4",
      track: "receiving.vtt",
      author: "Operations team",
      category: "Operations",
    },
    {
      id: "handover",
      title: "Make your next handover count",
      description: "Leave your teammates a clear next step.",
      duration: "12 sec",
      image: "team.svg",
      video: "handover.mp4",
      track: "handover.vtt",
      author: "People team",
      category: "Teamwork",
    },
    {
      id: "welcome",
      title: "A warm welcome, every time",
      description: "The details that customers remember.",
      duration: "12 sec",
      image: "store.svg",
      video: "welcome.mp4",
      track: "welcome.vtt",
      author: "Customer experience",
      category: "Sales",
    },
  ];
  seedExpansion();
}
function moduleAllowed(k) {
  return role().modules.includes(k);
}
function isVisible(item) {
  if (!item || item.holding !== state.holding || state.demoState === "expired")
    return false;
  if (item.module && !moduleAllowed(item.module)) return false;
  if (
    item.module === "people_payroll" &&
    !["owner", "finance"].includes(state.role)
  ) {
    if (!item.workflow || item.recordType !== "leave_request") return false;
    if (
      role().type === "staff" &&
      item.createdBy !== state.persona &&
      item.fields?.employee !== person().name
    )
      return false;
  }
  if (state.holding === "platform") return role().type === "platform";
  const a = assignment();
  if (!a) return false;
  if (item.branch && item.branch !== "all") {
    if (!a.branches.includes("all") && !a.branches.includes(item.branch))
      return false;
    if (state.branch !== "all" && item.branch !== state.branch) return false;
  }
  if (item.department && item.department !== "all") {
    if (
      state.department !== "all" &&
      item.department !== state.department &&
      item.kind === "post"
    )
      return false;
    if (
      item.kind === "post" &&
      !a.departments.includes("all") &&
      !a.departments.includes(item.department)
    )
      return false;
  }
  return true;
}
function visibleRecords(k) {
  return records.filter((r) => (!k || r.module === k) && isVisible(r));
}
function findRecord(k, id) {
  return records.find(
    (r) =>
      r.holding === state.holding &&
      r.module === k &&
      r.id === id &&
      isVisible(r),
  );
}
function visiblePosts() {
  return posts
    .filter(
      (p) =>
        isVisible(p) &&
        (!p.ref || findRecord(p.ref.module, p.ref.id)) &&
        referencesVisible(p.refs),
    )
    .sort((a, b) => (b.created || 0) - (a.created || 0));
}
function visibleStories() {
  return stories.filter(
    (s) =>
      isVisible(s) &&
      referencesVisible(s.refs) &&
      (!s.ref || findRecord(s.ref.module, s.ref.id)) &&
      Date.now() - s.created < 86400000,
  );
}
function canEdit(r) {
  return (
    !!r &&
    !recordReadOnly(r) &&
    isVisible(r) &&
    moduleAllowed(r.module) &&
    !(
      role().type === "staff" &&
      [
        "commercial",
        "customer_authority",
        "identity_access",
        "organization",
      ].includes(r.module)
    )
  );
}
function canTransition(r, action) {
  return (
    canEdit(r) &&
    !(
      r.module === "finance" &&
      state.periods[r.holding + ":" + primaryCompany(r)] === "Closed"
    ) &&
    (role().type !== "staff" ||
      (action &&
        !action[2] &&
        ["inventory", "support", "learning", "collaboration"].includes(
          r.module,
        )))
  );
}

window.ERPX_STATE = {
  savePrefs,
  scopeLabel,
  seed,
  visibleRecords,
  visiblePosts,
  visibleStories,
  canTransition,
};
