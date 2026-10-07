import type { AiEndpoint, AiMessage } from "@/server/modules/agent/public/ai-transport";
import { calculatePayrollAmounts, type PayrollComponentInput } from "@/lib/payroll-math";
import { decimalToMinor } from "@/lib/finance-money";

const simulationName = "thai-hotel-v1";

/** These selectors read process startup configuration only; request data cannot enable a mock. */
export function isThaiHotelBrowserSimulation() {
  return (
    process.env.NODE_ENV === "development" &&
    process.env.ERPX_ENVIRONMENT === "local" &&
    process.env.ERPX_BROWSER_SIMULATION === simulationName
  );
}

export function isThaiHotelAiMockEnabled(holdingSlug?: string) {
  return (
    isThaiHotelBrowserSimulation() &&
    holdingSlug === "riverside-lantern" &&
    process.env.ERPX_AI_ADAPTER === "deterministic-v1"
  );
}

export function isThaiHotelPayrollMockEnabled(holdingSlug?: string) {
  return (
    isThaiHotelBrowserSimulation() &&
    holdingSlug === "riverside-lantern" &&
    process.env.ERPX_PAYROLL_ADAPTER === "synthetic-fixtures-v1"
  );
}

export const thaiHotelAiMockId = "thai-hotel-v1-ai-deterministic-v1";
export const thaiHotelPayrollMockId = "thai-hotel-v1-payroll-fixtures-v1";

type MockAiInput = {
  result: Record<string, unknown>;
  inputTokens: number;
  outputTokens: number;
};

function contentText(message: AiMessage | undefined) {
  if (!message) return "";
  if (typeof message.content === "string") return message.content;
  return message.content
    .filter((part) => part.type === "text")
    .map((part) => (part.type === "text" ? part.text : ""))
    .join("\n");
}

function parseObject(value: string) {
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

/** Deterministic provider double. It supports only the two hotel-pilot prompts. */
export async function generateThaiHotelMockAi(
  endpoint: AiEndpoint,
  messages: AiMessage[],
): Promise<MockAiInput> {
  if (
    !isThaiHotelBrowserSimulation() ||
    process.env.ERPX_AI_ADAPTER !== "deterministic-v1" ||
    endpoint.baseUrl !== "simulation://thai-hotel-v1"
  )
    throw new Error("THAI_HOTEL_AI_MOCK_NOT_ENABLED");

  const system = contentText(messages.find((message) => message.role === "system"));
  const user = parseObject(contentText(messages.find((message) => message.role === "user")));
  let result: Record<string, unknown>;

  if (system.includes("Return a JSON object with ok equal to true")) {
    result = { ok: true, simulationAdapter: thaiHotelAiMockId };
  } else if (system.includes("Prepare suggestions for an employee request")) {
    const source = (user.source ?? {}) as Record<string, unknown>;
    const sourceTitle = typeof source.title === "string" ? source.title : "";
    result = {
      title: sourceTitle ? `Review draft: ${sourceTitle}`.slice(0, 200) : null,
      description:
        typeof source.description === "string" ? source.description : null,
      category: null,
      summary:
        "Simulation-only draft based on the saved request. A person must review and confirm it.",
      sources: ["request"],
    };
  } else if (system.includes("read-only work assistant and trainer")) {
    const sources = Array.isArray(user.sources)
      ? user.sources.filter(
          (source): source is Record<string, unknown> =>
            !!source && typeof source === "object" && !Array.isArray(source),
        )
      : [];
    const ids = sources
      .map((source) => source.id)
      .filter((id): id is string => typeof id === "string");
    const evidence = sources
      .map((source) => {
        const title = typeof source.title === "string" ? source.title : "record";
        const version = typeof source.version === "string" ? source.version : "unknown";
        return `${title} v${version.slice(0, 12)}`;
      })
      .join("; ");
    result = {
      answer: `Simulation-only summary for ${String(user.currentPage ?? "this workspace")}. Current authorized source snapshot: ${evidence || "no records"}. Review the linked records before closing the day; this response does not change any record.`,
      sourceIds: ids,
      followUps: ["Review each cited record before taking action."],
    };
  } else if (system.includes("ERPX's guided work intake")) {
    result = {
      intent: "unknown",
      summary: "Choose an available form to start this work.",
      draft: {},
      sourceIds: [],
    };
  } else {
    throw new Error("THAI_HOTEL_AI_MOCK_SCENARIO_UNSUPPORTED");
  }

  const inputTokens = Math.ceil(
    messages.reduce((total, message) => total + contentText(message).length, 0) / 4,
  );
  return {
    result,
    inputTokens,
    outputTokens: Math.ceil(JSON.stringify(result).length / 4),
  };
}

export type SyntheticPayrollInput = Parameters<typeof calculatePayrollAmounts>[0];
export type SyntheticPayrollComponent = PayrollComponentInput & {
  fixtureId: string;
};

/**
 * Use exact, named synthetic inputs supplied by the run's compensation fixtures.
 * No tax rates, statutory caps, or legal conclusions are calculated here.
 */
export function calculateThaiHotelPayrollFixture(input: {
  payroll: SyntheticPayrollInput;
  fixtures: SyntheticPayrollComponent[];
}) {
  const ordinary = calculatePayrollAmounts(input.payroll);
  const gross = ordinary.gross;
  let deductions = ordinary.deductions;
  let employer = ordinary.employer;
  const applied = [...ordinary.applied];
  const fixtures = input.fixtures.map((fixture) => {
    const amountMinor = decimalToMinor(fixture.amount);
    if (fixture.kind === "earning") {
      ordinary.gross += amountMinor;
    } else if (fixture.kind === "deduction") {
      deductions += amountMinor;
    } else {
      employer += amountMinor;
    }
    return { ...fixture, label: `MOCK ONLY · ${fixture.label}`, amountMinor };
  });
  const adjustedGross = gross + fixtures
    .filter((fixture) => fixture.kind === "earning")
    .reduce((total, fixture) => total + decimalToMinor(fixture.amount), 0);
  applied.push(...fixtures);
  return {
    gross: adjustedGross,
    deductions,
    employer,
    net: adjustedGross - deductions,
    overtimePay: ordinary.overtimePay,
    applied,
    adapter: {
      id: thaiHotelPayrollMockId,
      label: "Deterministic payroll fixture; not a Thai statutory calculation.",
      statutory: false as const,
    },
  };
}
