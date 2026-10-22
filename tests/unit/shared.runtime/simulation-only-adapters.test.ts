import { afterEach, describe, expect, it, vi } from "vitest";
import {
  calculateThaiHotelPayrollFixture,
  generateThaiHotelMockAi,
  isThaiHotelAiMockEnabled,
  isThaiHotelBrowserSimulation,
  isThaiHotelPayrollMockEnabled,
} from "@erpx/server-runtime/adapters/simulation-only-adapters";

afterEach(() => vi.unstubAllEnvs());

function enableBrowserSimulation() {
  vi.stubEnv("NODE_ENV", "development");
  vi.stubEnv("ERPX_ENVIRONMENT", "local");
  vi.stubEnv("ERPX_BROWSER_SIMULATION", "thai-hotel-v1");
  vi.stubEnv("ERPX_AI_ADAPTER", "deterministic-v1");
  vi.stubEnv("ERPX_PAYROLL_ADAPTER", "synthetic-fixtures-v1");
}

describe("Thai Hotel browser-simulation adapters", () => {
  it("cannot be enabled by a request or production environment configuration", () => {
    enableBrowserSimulation();
    expect(isThaiHotelBrowserSimulation()).toBe(true);
    expect(isThaiHotelAiMockEnabled("riverside-lantern")).toBe(true);
    expect(isThaiHotelAiMockEnabled("simulation")).toBe(false);
    expect(isThaiHotelPayrollMockEnabled("riverside-lantern")).toBe(true);
    expect(isThaiHotelPayrollMockEnabled("simulation")).toBe(false);

    vi.stubEnv("NODE_ENV", "production");
    expect(isThaiHotelBrowserSimulation()).toBe(false);
    expect(isThaiHotelAiMockEnabled("riverside-lantern")).toBe(false);
    expect(isThaiHotelPayrollMockEnabled("riverside-lantern")).toBe(false);
  });

  it("returns a repeatable human-review-only request draft", async () => {
    enableBrowserSimulation();
    const endpoint = {
      baseUrl: "simulation://thai-hotel-v1",
      model: "thai-hotel-v1-ai-deterministic-v1",
      apiKey: null,
    };
    const messages = [
      {
        role: "system" as const,
        content: "Prepare suggestions for an employee request. A human must review every suggestion.",
      },
      {
        role: "user" as const,
        content: JSON.stringify({
          source: {
            title: "Replace kitchen refrigerator",
            description: "The fictional kitchen refrigerator is noisy.",
          },
        }),
      },
    ];
    const first = await generateThaiHotelMockAi(endpoint, messages);
    const second = await generateThaiHotelMockAi(endpoint, messages);
    expect(first).toEqual(second);
    expect(first.result).toMatchObject({
      title: "Review draft: Replace kitchen refrigerator",
      description: "The fictional kitchen refrigerator is noisy.",
      category: null,
      sources: ["request"],
    });
    expect(first.result.summary).toContain("person must review and confirm");
  });

  it("cites the exact authorized source version in the day-close response", async () => {
    enableBrowserSimulation();
    const generated = await generateThaiHotelMockAi(
      {
        baseUrl: "simulation://thai-hotel-v1",
        model: "thai-hotel-v1-ai-deterministic-v1",
        apiKey: null,
      },
      [
        { role: "system", content: "You are ERPX's read-only work assistant and trainer." },
        {
          role: "user",
          content: JSON.stringify({
            currentPage: "/pms/night-audit?property=hotel-1",
            sources: [
              {
                id: "record:day-close",
                title: "Riverside day close",
                path: "/pms/night-audit?property=hotel-1",
                version: "0123456789abcdef",
                data: { blockers: ["cash session is open"] },
              },
            ],
            question: "What needs review before closing?",
          }),
        },
      ],
    );
    expect(generated.result.sourceIds).toEqual(["record:day-close"]);
    expect(generated.result.answer).toContain("Riverside day close v0123456789ab");
    expect(generated.result.answer).toContain("does not change any record");
  });

  it("applies named payroll fixture amounts without calculating statutory rates", () => {
    const result = calculateThaiHotelPayrollFixture({
      payroll: {
        employmentType: "salaried",
        baseMinor: 100_000,
        hourlyMinor: 0,
        regularHours: 160,
        overtimeHours: 0,
        components: [],
      },
      fixtures: [
        {
          fixtureId: "monthly-bonus",
          code: "MOCK_FIXTURE:bonus",
          label: "Synthetic bonus input",
          kind: "earning",
          amount: "20.00",
        },
        {
          fixtureId: "monthly-withholding",
          code: "MOCK_FIXTURE:withholding",
          label: "Synthetic withholding input",
          kind: "deduction",
          amount: "12.50",
        },
        {
          fixtureId: "monthly-employer-cost",
          code: "MOCK_FIXTURE:employer-contribution",
          label: "Synthetic employer contribution input",
          kind: "employer",
          amount: "2.50",
        },
      ],
    });
    expect(result).toMatchObject({
      gross: 102_000,
      deductions: 1_250,
      employer: 250,
      net: 100_750,
      adapter: {
        id: "thai-hotel-v1-payroll-fixtures-v1",
        statutory: false,
      },
    });
    expect(result.applied.map((component) => component.label)).toContain(
      "MOCK ONLY · Synthetic withholding input",
    );
  });
});
