import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  allowedWidgets,
  defaultLayout,
  featureLocation,
  layoutDifferences,
  legacyWidgetTarget,
  pageLayoutSchema,
  reconcileLayout,
  resetWidgetToOrganization,
} from "../src/lib/page-layouts";
import { pageLayoutRequest } from "../src/server/page-layouts";
import { applyMigrations, env } from "./runtime-env";
let user = "",
  sessionId = "",
  assignment = "";
vi.mock("../src/server/runtime", () => ({ runtimeEnv: async () => env }));
vi.mock("../src/server/authorization", () => ({
  currentSession: async () => ({
    session: { user: { id: user }, session: { id: sessionId } },
  }),
  requirePlatformContext: async () => ({
    userId: user,
    assignmentId: assignment,
    role: "platform_admin",
  }),
  requireWorkContext: async () => {
    throw Error("Unexpected business authorization");
  },
  publicScope: () => {
    throw Error("Unexpected business scope");
  },
}));
beforeAll(() => applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS));
beforeEach(async () => {
  user = crypto.randomUUID();
  sessionId = crypto.randomUUID();
  assignment = crypto.randomUUID();
  const now = new Date().toISOString();
  await env.CONTROL_DB.prepare(
    'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,0,?,?)',
  )
    .bind(user, "Layout test", `${user}@example.test`, Date.now(), Date.now())
    .run();
  await env.CONTROL_DB.prepare(
    'INSERT INTO session(id,"userId",token,"expiresAt","createdAt","updatedAt") VALUES(?,?,?,?,?,?)',
  )
    .bind(
      sessionId,
      user,
      crypto.randomUUID(),
      new Date(Date.now() + 3600000).toISOString(),
      now,
      now,
    )
    .run();
  await env.CONTROL_DB.prepare(
    "INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,?,?,?,?)",
  )
    .bind(
      assignment,
      user,
      "platform_admin",
      "[]",
      Date.now() - 1000,
      Date.now() + 3600000,
    )
    .run();
});
const layout = (hidden = false) => ({
  schemaVersion: 1,
  widgets: [
    { id: "account-security-controls", columns: 6, height: 320, hidden },
  ],
});
async function request(
  body?: unknown,
  key = crypto.randomUUID(),
  page = "account-security",
  account = true,
) {
  const response = await pageLayoutRequest(
    new Request(
      `http://localhost:3100/api/v1/me/page-layouts/${page}`,
      body
        ? {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": key,
            },
            body: JSON.stringify(body),
          }
        : undefined,
    ),
    page,
    undefined,
    account,
  );
  return { status: response.status, ...(await response.json()) };
}
it("persists across sessions and resets without saving business data", async () => {
  expect((await request()).data.personal).toBeNull();
  expect(
    (
      await request({
        action: "save",
        expectedVersion: 0,
        layout: layout(true),
      })
    ).data.version,
  ).toBe(1);
  expect((await request()).data.personal.widgets[0]).toMatchObject({
    hidden: true,
    columns: 6,
  });
  expect(
    (await request({ action: "reset", expectedVersion: 1, layout: null })).data,
  ).toMatchObject({ personal: null, version: 2 });
  expect(
    pageLayoutSchema.safeParse({
      ...layout(),
      businessData: { draft: "secret" },
    }).success,
  ).toBe(false);
});
it("rejects concurrent saves, replays identical commands, and audits once", async () => {
  const key = crypto.randomUUID(),
    body = { action: "save", expectedVersion: 0, layout: layout() };
  const first = await request(body, key);
  expect(first.status).toBe(200);
  expect(await request(body, key)).toEqual(first);
  expect((await request({ ...body, layout: layout(true) }, key)).status).toBe(
    409,
  );
  expect((await request({ ...body, layout: layout(true) })).status).toBe(409);
  expect(
    (
      await env.CONTROL_DB.prepare(
        "SELECT count(*) n FROM page_layout_receipts WHERE actor_id=?",
      )
        .bind(user)
        .first<{ n: number }>()
    )?.n,
  ).toBe(1);
});
it("isolates account owners and rejects unknown widgets and cross-scope pages", async () => {
  await request({ action: "save", expectedVersion: 0, layout: layout() });
  user = crypto.randomUUID();
  expect((await request()).data.personal).toBeNull();
  expect(
    (
      await request({
        action: "save",
        expectedVersion: 0,
        layout: {
          ...layout(),
          widgets: [{ ...layout().widgets[0], id: "pos-company-stripe" }],
        },
      })
    ).status,
  ).toBe(403);
  expect((await request(undefined, undefined, "platform-home")).status).toBe(
    403,
  );
});
it("publishes organization revisions while retaining personal layouts until reset", async () => {
  const organization = {
    schemaVersion: 1,
    widgets: [
      { id: "platform-workspaces", columns: 12, height: null, hidden: false },
    ],
  };
  const call = (body?: unknown) =>
    request(body, crypto.randomUUID(), "platform-home", false);
  expect(
    (
      await call({
        action: "publish",
        expectedVersion: 0,
        layout: organization,
      })
    ).data.organizationVersion,
  ).toBe(1);
  await call({
    action: "save",
    expectedVersion: 0,
    layout: {
      ...organization,
      widgets: [{ ...organization.widgets[0], columns: 6 }],
    },
  });
  const newer = await call({
    action: "publish",
    expectedVersion: 1,
    layout: {
      ...organization,
      widgets: [{ ...organization.widgets[0], hidden: true }],
    },
  });
  expect(newer.data.personal.widgets[0].columns).toBe(6);
  expect(newer.data.organization.widgets[0].hidden).toBe(true);
  expect(
    (await call({ action: "reset", expectedVersion: 1, layout: null })).data
      .personal,
  ).toBeNull();
});
it("refuses mutation after session revocation", async () => {
  await env.CONTROL_DB.prepare("DELETE FROM session WHERE id=?")
    .bind(sessionId)
    .run();
  expect(
    (await request({ action: "save", expectedVersion: 0, layout: layout() }))
      .status,
  ).toBe(401);
});
it("keeps detail layouts record independent and legacy links focusable", () => {
  const context = {
    workspaceBase: "/holdings/acme",
    role: "holding_owner",
    scopeKind: "holding" as const,
  };
  expect(
    featureLocation(context, "/holdings/acme/people/a", new URLSearchParams())
      .pageKey,
  ).toBe(
    featureLocation(context, "/holdings/acme/people/b", new URLSearchParams())
      .pageKey,
  );
  expect(legacyWidgetTarget(new URLSearchParams("status=archived"))).toBe(
    "archived-profiles",
  );
  expect(legacyWidgetTarget(new URLSearchParams("section=attributes"))).toBe(
    "attributes",
  );
  const widgets = allowedWidgets("people-directory", context),
    original = defaultLayout(widgets);
  original.widgets[0].hidden = true;
  expect(reconcileLayout(original, widgets).widgets[0].hidden).toBe(true);
  expect(
    allowedWidgets("pos-settings", {
      ...context,
      role: "employee",
      permissions: [],
    }),
  ).toEqual([]);
  expect(
    allowedWidgets("people-directory", {
      ...context,
      role: "employee",
      permissions: [],
    }).some((w) => w.id === "people-explorer"),
  ).toBe(false);
});
it("keeps saved commerce layouts after customer and supplier pages move", () => {
  const context = {
    workspaceBase: "/holdings/acme",
    role: "holding_owner",
    scopeKind: "holding" as const,
  };
  for (const kind of ["customer", "supplier"]) {
    const location = featureLocation(
      context,
      "/holdings/acme/commerce",
      new URLSearchParams(`view=parties&kind=${kind}`),
    );
    expect(location.pageKey).toBe("operations-parties");
    expect(allowedWidgets(location.pageKey, context).length).toBeGreaterThan(0);
  }
  expect(allowedWidgets("commerce-orders-detail", context).length).toBeGreaterThan(0);
});
it("keeps standard People in one workspace while leaving Beta People navigation unchanged", () => {
  const locate = (context: Parameters<typeof featureLocation>[0], path = "people") =>
    featureLocation(context, `${context.workspaceBase}/${path}`, new URLSearchParams());
  const owner = locate({
    workspaceBase: "/holdings/acme",
    scopeKind: "holding",
    role: "holding_owner",
    adminSections: ["people", "organization"],
  });
  expect(owner.tabs).toEqual([]);
  expect(owner.secondaryTabs).toEqual([]);

  const employee = locate({ workspaceBase: "/holdings/acme", role: "employee" });
  expect(employee.tabs).toEqual([]);
  expect(employee.secondaryTabs).toEqual([]);

  const payrollReader = locate({
    workspaceBase: "/holdings/acme",
    role: "employee",
    permissions: ["people.read", "payroll.read"],
  });
  expect(payrollReader.tabs).toEqual([]);
  expect(payrollReader.secondaryTabs).toEqual([]);
  const branch = locate({
    workspaceBase: "/holdings/acme/branches/main",
    scopeKind: "branch",
    role: "holding_owner",
  });
  expect(branch.tabs).toEqual([]);
  expect(branch.secondaryTabs).toEqual([]);

  const beta = locate({
    workspaceBase: "/holdings/beta",
    scopeKind: "holding",
    beta: true,
    role: "holding_owner",
  });
  expect(beta.tabs.map((tab) => tab.id)).toEqual([
    "people-directory",
    "people-organization",
    "people-access",
  ]);
  expect(beta.secondaryTabs).toEqual([]);
});
it("describes personal layout changes and resets one widget to its organization position", () => {
  const definitions = [
    { id: "a", title: "A", page: "test", columns: 6 },
    { id: "b", title: "B", page: "test", columns: 12 },
    { id: "c", title: "C", page: "test", columns: 12 },
  ];
  const organization = defaultLayout(definitions);
  organization.widgets[0].height = 300;
  const personal = {
    schemaVersion: 1 as const,
    widgets: [
      { ...organization.widgets[2] },
      { ...organization.widgets[1], hidden: true },
      { ...organization.widgets[0], columns: 12, height: null },
    ],
  };
  expect(layoutDifferences(null, organization, definitions)).toEqual({
    hasPersonalOverride: false,
    changeCount: 0,
    widgets: [],
  });
  const comparison = layoutDifferences(personal, organization, definitions);
  expect(comparison.changeCount).toBe(3);
  expect(comparison.widgets.find((widget) => widget.id === "a")?.badges).toEqual(
    expect.arrayContaining(["Moved", "Width 6/12 → 12/12", "Fixed height → Auto"]),
  );
  expect(comparison.widgets.find((widget) => widget.id === "b")?.badges).toContain("Hidden");

  const reset = resetWidgetToOrganization(personal, "b", organization, definitions);
  expect(reset.widgets.map((widget) => widget.id)).toEqual(["c", "b", "a"]);
  expect(reset.widgets.find((widget) => widget.id === "b")).toEqual(organization.widgets[1]);
  expect(reset.widgets.find((widget) => widget.id === "a")?.columns).toBe(12);
});
