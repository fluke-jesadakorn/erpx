import { beforeEach, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));
vi.mock("react", () => ({ cache: (fn: unknown) => fn }));

const mocks = vi.hoisted(() => ({
  session: vi.fn(),
  platform: vi.fn(),
  bindings: vi.fn(),
  resources: vi.fn(),
  role: vi.fn(),
}));

vi.mock("../src/server/authorization", () => ({
  currentSession: mocks.session,
  requirePlatformContext: mocks.platform,
}));
vi.mock("../src/server/runtime", () => ({
  activeHoldingBindings: mocks.bindings,
  holdingResources: mocks.resources,
}));
vi.mock("../src/server/iam-store", () => ({ iamRoles: async (db:unknown, companyId:string, ids:string[]) => new Map(await Promise.all(ids.map(async id=>[id,await mocks.role(db,companyId,id)] as const))) }));

import { workspaceChoices } from "../src/server/workspace-choices";
import {
  buildWorkspaceHoldingChoice,
  type WorkspaceChoiceAssignment,
  type WorkspaceChoiceNode,
} from "../src/lib/workspace-choice";
import {
  capabilitySupport,
  liveDestination,
  moduleAvailability,
  moduleRegistry,
} from "../src/data/module-registry";

function database(assignments: unknown[], nodes: unknown[]) {
  return {
    prepare: (sql: string) => ({
      bind: () => ({
        all: async () => ({
          results: sql.includes("FROM assignments") ? assignments : nodes,
        }),
      }),
    }),
  };
}

beforeEach(() => {
  mocks.session
    .mockReset()
    .mockResolvedValue({ session: { user: { id: "user" } } });
  mocks.platform.mockReset().mockRejectedValue(new Error("FORBIDDEN"));
  mocks.bindings.mockReset().mockResolvedValue([]);
  mocks.resources.mockReset();
  mocks.role.mockReset().mockImplementation(
    async (_db: unknown, _companyId: string, role: string) => ({
      scopes:
        role === "department-only"
          ? ["department"]
          : ["holding", "branch", "department"],
    }),
  );
});

it("platform authority alone creates no business choices", async () => {
  mocks.platform.mockResolvedValue({});
  expect(await workspaceChoices()).toEqual([]);
  expect(mocks.platform).not.toHaveBeenCalled();
});

it("sorts multiple Holdings by label and slug", async () => {
  mocks.bindings.mockResolvedValue([
    { id: "z", slug: "zeta", displayName: "Shared" },
    { id: "a", slug: "alpha", displayName: "Shared" },
  ]);
  mocks.resources.mockImplementation(async (holdingId: string) => ({
    write: database(
      [
        {
          role: "owner",
          company_id: `${holdingId}-company`,
          company_name: `${holdingId.toUpperCase()} Company`,
          scope_kind: "holding",
          scope_node_id: null,
        },
      ],
      [],
    ),
  }));

  expect(
    (await workspaceChoices()).map((choice) =>
      choice.kind === "holding" ? choice.holdingSlug : choice.kind,
    ),
  ).toEqual(["alpha", "zeta"]);
});

it("builds Holding, Company and Branch choices from active assignments", async () => {
  mocks.bindings.mockResolvedValue([
    { id: "h001", slug: "acme", displayName: "Acme Group" },
  ]);
  mocks.resources.mockResolvedValue({
    write: database(
      [
        {
          role: "owner",
          company_id: "acme-co",
          company_name: "Acme Company",
          scope_kind: "holding",
          scope_node_id: null,
        },
        {
          role: "operator",
          company_id: "beacon-co",
          company_name: "Beacon Labs",
          scope_kind: "branch",
          scope_node_id: "beacon-branch",
        },
      ],
      [
        {
          id: "z-branch",
          kind: "branch",
          slug: "z-branch",
          display_name: "Z Branch",
          company_id: "acme-co",
          branch_id: null,
          branch_slug: null,
        },
        {
          id: "a-branch",
          kind: "branch",
          slug: "a-branch",
          display_name: "A Branch",
          company_id: "acme-co",
          branch_id: null,
          branch_slug: null,
        },
        {
          id: "beacon-branch",
          kind: "branch",
          slug: "lab",
          display_name: "Lab",
          company_id: "beacon-co",
          branch_id: null,
          branch_slug: null,
        },
      ],
    ),
  });

  const [holding] = await workspaceChoices();
  expect(holding).toMatchObject({
    kind: "holding",
    holdingSlug: "acme",
    label: "Acme Group",
  });
  if (holding.kind !== "holding") throw new Error("holding choice expected");
  expect(holding.companies.map((company) => company.label)).toEqual([
    "Acme Company",
    "Beacon Labs",
  ]);
  expect(holding.companies[0]).toMatchObject({
    id: "acme-co",
    destination: "/holdings/acme/home",
    switchEndpoint: "/api/v1/holdings/acme/companies/active",
  });
  expect(holding.companies[0].branches.map((branch) => branch.label)).toEqual([
    "A Branch",
    "Z Branch",
  ]);
  expect(holding.companies[1].destination).toBeUndefined();
  expect(holding.companies[1].branches[0]).toMatchObject({
    label: "Lab",
    destination: "/holdings/acme/branches/lab/home",
  });
});

it("shows only direct Department grants when no selectable parent exists", () => {
  const assignments: WorkspaceChoiceAssignment[] = [
    {
      companyId: "company",
      companyName: "Company",
      scopeKind: "department",
      scopeNodeId: "department",
    },
  ];
  const nodes: WorkspaceChoiceNode[] = [
    {
      id: "branch",
      kind: "branch",
      slug: "bangkok",
      label: "Bangkok",
      companyId: "company",
      branchId: null,
      branchSlug: null,
    },
    {
      id: "department",
      kind: "department",
      slug: "operations",
      label: "Operations",
      companyId: "company",
      branchId: "branch",
      branchSlug: "bangkok",
    },
    {
      id: "other-department",
      kind: "department",
      slug: "finance",
      label: "Finance",
      companyId: "company",
      branchId: "branch",
      branchSlug: "bangkok",
    },
  ];
  const holding = buildWorkspaceHoldingChoice(
    "acme",
    "Acme",
    assignments,
    nodes,
  );
  expect(holding?.companies[0].destination).toBeUndefined();
  expect(holding?.companies[0].branches[0].destination).toBeUndefined();
  expect(holding?.companies[0].branches[0].departments).toEqual([
    {
      kind: "department",
      id: "department",
      label: "Operations",
      destination:
        "/holdings/acme/branches/bangkok/departments/operations/home",
    },
  ]);
});

it("does not repeat Department choices beneath an accessible parent", () => {
  const holding = buildWorkspaceHoldingChoice(
    "acme",
    "Acme",
    [
      {
        companyId: "company",
        companyName: "Company",
        scopeKind: "branch",
        scopeNodeId: "branch",
      },
      {
        companyId: "company",
        companyName: "Company",
        scopeKind: "department",
        scopeNodeId: "department",
      },
    ],
    [
      {
        id: "branch",
        kind: "branch",
        slug: "bangkok",
        label: "Bangkok",
        companyId: "company",
        branchId: null,
        branchSlug: null,
      },
      {
        id: "department",
        kind: "department",
        slug: "operations",
        label: "Operations",
        companyId: "company",
        branchId: "branch",
        branchSlug: "bangkok",
      },
    ],
  );
  expect(holding?.companies[0].branches[0].destination).toBe(
    "/holdings/acme/branches/bangkok/home",
  );
  expect(holding?.companies[0].branches[0].departments).toEqual([]);
});

it("does not mask unexpected Holding discovery failures", async () => {
  mocks.bindings.mockRejectedValue(new Error("DATABASE_UNAVAILABLE"));
  await expect(workspaceChoices()).rejects.toThrow("DATABASE_UNAVAILABLE");
});

it("all platform sections report previews with no live action destinations", () => {
  for (const entry of moduleRegistry) {
    expect(moduleAvailability(entry, "/platform")).toBe("preview");
    for (const section of entry.sections) {
      expect(liveDestination(entry.id, section.id, "/platform")).toBeUndefined();
      const support = capabilitySupport(
        entry.id,
        section.id,
        "/platform",
      );
      expect(support.availability).toBe("preview");
      expect(
        support.actions.every(
          (action) => action.availability === "preview" && !action.destination,
        ),
      ).toBe(true);
    }
  }
  expect(
    liveDestination("finance", "receivables", "/holdings/acme"),
  ).toBe("finance/invoices?kind=receivable");
});
