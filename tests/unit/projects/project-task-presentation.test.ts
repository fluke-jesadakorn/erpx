import { afterEach, beforeEach, expect, it, vi } from "vitest";
const request = vi.hoisted(() => ({ host: "work-dev-erpx.jesadakorn.com", actor: { companyId: "company", assignmentId: "assignment" }, project: vi.fn(), quality: vi.fn() }));
vi.mock("next/headers", () => ({ headers: async () => new Headers({ host: request.host }) }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requireWorkContext: async () => request.actor }));
vi.mock("@erpx/server-projects/public/product-entry", () => ({ projectTaskSourceRecordBackdrop: request.project }));
vi.mock("@erpx/server-quality/public/product-entry", () => ({ qualitySourceRecordBackdrop: request.quality }));
import { projectTaskProductPresentationOptions } from "@erpx/server-projects/public/product-presentation-request";
import { sourceRecordPresentation } from "@erpx/feature-records/components/source-record-presentation";

beforeEach(() => {
  vi.stubEnv("ERPX_ENVIRONMENT", "local"); vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT", "development"); vi.stubEnv("ERPX_PRODUCT_ROUTING", "enabled"); vi.stubEnv("ERPX_PRODUCT_SSO", "enabled");
  vi.clearAllMocks(); request.host = "work-dev-erpx.jesadakorn.com";
  request.project.mockResolvedValue({ kind: "task", title: "Actual Project source", status: "ready" });
  request.quality.mockResolvedValue({ kind: "hold", title: "Actual Quality source", status: "open" });
});
afterEach(() => vi.unstubAllEnvs());
it("tightens only actual configured Work-host Project Task presentation", async () => {
  expect(await projectTaskProductPresentationOptions()).toEqual({ strictProduct: true });
  for (const host of ["console-dev-erpx.jesadakorn.com", "projects-dev-erpx.jesadakorn.com", "work-dev-erpx.jesadakorn.com.evil.test", "work-dev-erpx.jesadakorn.com:444"]) {
    request.host = host; expect(await projectTaskProductPresentationOptions()).toEqual({});
  }
});
it("passes the strict request option through the actual canonical source background", async () => {
  expect(await sourceRecordPresentation("acme", "staff_task", "task")).toEqual({ kind: "task", title: "Actual Project source", status: "ready" });
  expect(request.project).toHaveBeenCalledWith(request.actor, { kind: "task", id: "task" }, { strictProduct: true });
  expect(request.quality).not.toHaveBeenCalled();
});
it("preserves Console defaults and the independent strict Supply source branch", async () => {
  request.host = "console-dev-erpx.jesadakorn.com";
  await sourceRecordPresentation("acme", "staff_task", "task");
  expect(request.project).toHaveBeenLastCalledWith(request.actor, { kind: "task", id: "task" }, {});
  request.host = "supply-dev-erpx.jesadakorn.com";
  await sourceRecordPresentation("acme", "quality_hold", "hold");
  expect(request.quality).toHaveBeenCalledWith(request.actor, { kind: "hold", id: "hold" }, { strictProduct: true });
  expect(request.project).toHaveBeenCalledTimes(1);
});
