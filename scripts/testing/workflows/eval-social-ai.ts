import cases from "../../../fixtures/ai/social-beta-cases.json";
import { callAi } from "@erpx/server-collaboration/public/social-ai";
const provider = {
  AI_BASE_URL: process.env.AI_BASE_URL,
  AI_MODEL: process.env.AI_MODEL,
  AI_API_KEY: process.env.AI_API_KEY,
};
if (!provider.AI_BASE_URL || !provider.AI_MODEL || !provider.AI_API_KEY)
  throw new Error(
    "Configure AI_BASE_URL, AI_MODEL and AI_API_KEY to run the live provider evaluation.",
  );
const results = [];
for (const sample of cases) {
  try {
    const output = await callAi(provider, {
      kind: sample.kind,
      sources: [{ id: sample.id, body: sample.body, version: 1 }],
      roster: [
        { id: "alex-1", name: "Alex / นัท" },
        { id: "alex-2", name: "Alex / นัท" },
        { id: "mali", name: "Mali / มะลิ" },
      ],
    });
    const nulls = sample.mustBeNull.every(
      (field) =>
        output.result[
          field as "assigneeUserId" | "reviewerUserId" | "dueAt"
        ] === null,
    );
    const expected =
      !sample.expected ||
      Object.entries(sample.expected).every(([field, value]) =>
        field === "dueAt"
          ? Date.parse(output.result.dueAt ?? "") === Date.parse(String(value))
          : output.result.assigneeUserId === value,
      );
    results.push({
      id: sample.id,
      passed:
        nulls &&
        expected &&
        output.result.citations.every((id) => id === sample.id) &&
        output.result.missing.length > 0,
      inputTokens: output.inputTokens,
      outputTokens: output.outputTokens,
    });
  } catch {
    results.push({
      id: sample.id,
      passed: false,
      error: "Provider or schema failure",
    });
  }
}
const report = {
  model: provider.AI_MODEL,
  ranAt: new Date().toISOString(),
  passed: results.filter((r) => r.passed).length,
  total: results.length,
  results,
};
await Bun.write(
  ".scratch/social-ai-evaluation.json",
  JSON.stringify(report, null, 2),
);
console.log(
  `${report.passed}/${report.total} live provider cases passed. Evidence: .scratch/social-ai-evaluation.json`,
);
if (report.passed !== report.total) process.exitCode = 1;
