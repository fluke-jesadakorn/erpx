import { oasisDemoHolding } from "@erpx/platform-contracts/holding-hosts";

type Command = Record<string, unknown>;
type CommandTransport = { path: string; body: string; receiptAtRoot: boolean };
const demoOrigin = `https://${oasisDemoHolding.hostname}`;
const holdingApi = `/api/v1/holdings/${oasisDemoHolding.holdingSlug}`;
const crmScope = /^(?:\/branches\/[a-z0-9_-]{1,128})?(?:\/departments\/[a-z0-9_-]{1,128})?\/crm(\/activities)?$/;
const leadActions = new Set(["qualify", "reassign", "disqualify", "reopen"]);
const opportunityActions = new Set(["link-quote", "negotiate", "win", "win-pms", "lose", "reopen"]);

export function crmCommandTransport(
  origin: string,
  path: string,
  input: Command,
  originalBody: string,
): CommandTransport | undefined {
  if (origin !== demoOrigin) return { path, body: originalBody, receiptAtRoot: false };
  const scope = path.startsWith(holdingApi) ? crmScope.exec(path.slice(holdingApi.length)) : null;
  if (!scope) return undefined;

  const { id, domain, ...fields } = input;
  const idSegment = typeof id === "string" && id !== "" && id !== "." && id !== ".."
    ? encodeURIComponent(id)
    : undefined;
  if (scope[1]) {
    if (id === undefined) return { path, body: originalBody, receiptAtRoot: false };
    if (!idSegment) return undefined;
    const { action, expectedVersion, reason, ownerId, dueDate } = fields;
    if (action !== "complete" && action !== "cancel" && action !== "reassign") return undefined;
    const body = JSON.stringify({
      action, expectedVersion, reason,
      ...(action === "reassign" ? { ownerId, dueDate } : {}),
    });
    return { path: `${path}/${idSegment}/changes`, body, receiptAtRoot: false };
  }

  if (!idSegment || typeof fields.action !== "string") return undefined;
  const leadPath = `${path}/leads/${idSegment}`;
  if (domain === undefined && leadActions.has(fields.action)) {
    return { path: `${leadPath}/changes`, body: JSON.stringify(fields), receiptAtRoot: true };
  }
  if (domain === "opportunity" && opportunityActions.has(fields.action)) {
    return {
      path: `${leadPath}/opportunity/changes`,
      body: JSON.stringify(fields),
      receiptAtRoot: true,
    };
  }
  return undefined;
}
