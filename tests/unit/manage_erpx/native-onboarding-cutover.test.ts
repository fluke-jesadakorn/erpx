import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));
vi.mock("next/headers", () => ({ headers: async () => new Headers() }));

import {
  NativeManageErpxError,
  createNativeManageErpxClient,
  nativeManageErpxConfiguration,
  nativeManageErpxFailure,
  type NativeManageErpxTransport,
} from "../../../packages/feature-platform_administration/src/adapters/server/native-manage-erpx";
import {
  canonicalDestinationLink,
  canonicalDestinationLinks,
} from "@erpx/feature-work_projection/lib/destination-links";

const route = new URL(
  "../../../packages/workplace/src/routes/api/v1/onboarding/route.ts",
  import.meta.url,
);
const reexport = new URL(
  "../../../apps/compat/src/app/api/v1/onboarding/route.ts",
  import.meta.url,
);
const source = (path: string) =>
  readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");

const config = nativeManageErpxConfiguration(
  JSON.stringify({
    privateBaseUrl: "http://127.0.0.1:3301",
    hosts: ["https://acme.example.test"],
    timeoutMs: 1_000,
  }),
);

function transport(body: unknown, status = 200) {
  const fn = vi.fn(async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json; charset=utf-8" },
    }),
  );
  return fn as unknown as NativeManageErpxTransport;
}

const incoming = new Headers({ host: "acme.example.test", cookie: "erpx=synthetic" });

describe("native onboarding public cutover", () => {
  it("retired the Next route and its compat re-export", () => {
    expect(existsSync(route), route.pathname).toBe(false);
    expect(existsSync(reexport), reexport.pathname).toBe(false);
  });

  it("keeps the request guard and response envelope in the owner adapters", () => {
    const read = source(
      "packages/server-manage_erpx/src/adapters/onboarding_http.rs",
    );
    expect(read).toContain("fn valid_host");
    expect(read).toContain("fn unique_header");
    expect(read).toContain("fn bounded");
    expect(read).toContain("fn error_response");
    expect(read).toContain("StatusCode::UNAUTHORIZED");
    expect(read).toContain("StatusCode::SERVICE_UNAVAILABLE");
    expect(read).toContain("\"data\": self.data, \"context\": self.context");

    const write = source(
      "packages/server-manage_erpx/src/adapters/onboarding_command_http.rs",
    );
    expect(write).toContain("pub const MANAGE_ERPX_COMMAND_BODY_LIMIT");
    expect(write).toContain("Ok(reply) => bounded(reply.value())");

    const input = source(
      "packages/server-manage_erpx/src/domain/onboarding_command.rs",
    );
    expect(input).toContain("pub fn from_json(body: &Value)");
    expect(input).toContain("the retired Zod schema");
  });

  it("serves the owner view and receipt in the browser envelope without canonicalisation", async () => {
    const view = {
      enabled: true,
      slot: {
        slug: "acme",
        company_name: "Acme Company",
        branch_name: "Main",
        state: "provisioning",
      },
    };
    const viewClient = createNativeManageErpxClient(
      config,
      transport({ data: view, context: { userId: "owner" } }),
    );
    expect(await viewClient.onboardingView(incoming, "owner")).toEqual(view);
    expect(canonicalDestinationLinks({ data: view }).data).toEqual(view);

    const receipt = { state: "active", href: "/holdings/acme/home" } as const;
    const commandClient = createNativeManageErpxClient(
      config,
      transport({ data: receipt, context: { userId: "owner" } }),
    );
    expect(await commandClient.onboardingCommand(incoming, {
      companyName: "Acme Company",
      branchName: "Main",
      slug: "acme",
    })).toEqual(receipt);
    expect(canonicalDestinationLinks({ data: receipt }).data.href).toBe(receipt.href);
    expect(canonicalDestinationLink(receipt.href)).toBe(receipt.href);
  });

  it("maps owner errors to the retired route envelope", () => {
    const cases = [
      ["UNAUTHENTICATED", 401, "Sign in to continue."],
      ["VERIFY_EMAIL", 403, "Verify your email before opening a Company."],
      ["ONBOARDING_ALREADY_STARTED", 409, "Company already started."],
      ["MANAGE_ERPX_REQUEST_INVALID", 422, "Use one exact registered application host."],
    ] as const;
    for (const [code, status, message] of cases) {
      expect(nativeManageErpxFailure(new NativeManageErpxError(code, status, message)))
        .toEqual({ status, code, message });
    }
    // The retired Next route normalised every 503 to the same bounded envelope.
    for (const code of [
      "MANAGE_ERPX_UNAVAILABLE",
      "NATIVE_MANAGE_ERPX_RESPONSE_INVALID",
      "NATIVE_MANAGE_ERPX_RESPONSE_LIMIT",
      "NATIVE_MANAGE_ERPX_CONFIGURATION_INVALID",
    ]) {
      expect(
        nativeManageErpxFailure(new NativeManageErpxError(code, 503, "owner detail")),
      ).toEqual({
        status: 503,
        code: "NATIVE_MANAGE_ERPX_UNAVAILABLE",
        message: "The native Manage ERPX service is unavailable.",
      });
    }
  });
});
