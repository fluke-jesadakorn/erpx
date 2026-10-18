import type { CapabilityPurpose } from "../../commercial/public/packages";

const newWork = new Set(["support.create", "support.reopen", "support.incident.create", "support.incident.reopen",
  "support.session.request", "support.session.approve", "support.session.issue-capability", "support.sla.create", "support.sla.approve", "support.sla.attach"]);
const settlement = new Set(["support.triage", "support.start", "support.wait-customer", "support.resume", "support.resolve", "support.reject-resolution",
  "support.confirm", "support.confirm-general", "support.cancel", "support.reassign", "support.edit-report", "support.withdraw-report", "support.add-information",
  "support.incident.start", "support.incident.update", "support.incident.mitigate", "support.incident.reject", "support.incident.close", "support.incident.cancel",
  "support.incident.reassign", "support.incident.link", "support.incident.unlink", "support.session.reject", "support.session.revoke", "support.session.revoke-capability",
  "support.sla.retire", "support.sla.acknowledge", "support.evidence.bind", "support.evidence.revoke"]);

/** Deliberate classification: a new command never silently becomes settlement. */
export function supportCommandPurchase(operation: string): { capability: string; purpose: CapabilityPurpose } {
  if (!newWork.has(operation) && !settlement.has(operation)) throw new Error(`SUPPORT_PACKAGE_COMMAND_REQUIRED:${operation}`);
  const capability = operation.startsWith("support.incident.") ? "support.incidents" : operation.startsWith("support.sla.") ? "support.sla"
    : operation.startsWith("support.evidence.") ? "support.evidence" : "support.tickets";
  return { capability, purpose: newWork.has(operation) ? "new-work" : "settle-existing" };
}
