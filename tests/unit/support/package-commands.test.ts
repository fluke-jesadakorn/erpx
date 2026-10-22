import { expect, it } from "vitest";
import { supportCommandPurchase } from "@erpx/server-support/domain/package-commands";
import { supportChangeSchema } from "@erpx/server-support/domain/support";
import { incidentChangeSchema } from "@erpx/server-support/domain/support-incidents";
import { supportSessionActionSchema } from "@erpx/server-support/domain/support-sessions";
import { supportPolicyChangeSchema } from "@erpx/server-support/domain/support-sla";

it("requires an explicit package classification for every current owner command schema", () => {
  const commands = ["support.create", "support.incident.create", "support.session.request", "support.sla.create", "support.sla.attach", "support.sla.acknowledge", "support.evidence.bind", "support.evidence.revoke"];
  for (const [prefix, schemas] of [["support", supportChangeSchema.options], ["support.incident", incidentChangeSchema.options], ["support.session", supportSessionActionSchema.options]] as const) {
    for (const schema of schemas) {
      const action = schema.shape.action;
      const actions = "options" in action ? action.options : [action.value];
      for (const value of actions) commands.push(`${prefix}.${value}`);
    }
  }
  commands.push(...supportPolicyChangeSchema.shape.action.options.map(action => `support.sla.${action}`));
  for (const operation of commands) expect(() => supportCommandPurchase(operation)).not.toThrow();
  expect(() => supportCommandPurchase("support.future-command")).toThrow("SUPPORT_PACKAGE_COMMAND_REQUIRED");
});

it("keeps access expansion and reopening as new work while allowing existing obligation settlement", () => {
  for (const operation of ["support.create", "support.reopen", "support.incident.create", "support.incident.reopen", "support.session.request", "support.session.approve", "support.session.issue-capability", "support.sla.create", "support.sla.approve", "support.sla.attach"])
    expect(supportCommandPurchase(operation).purpose).toBe("new-work");
  for (const operation of ["support.confirm-general", "support.cancel", "support.incident.close", "support.session.reject", "support.session.revoke", "support.session.revoke-capability", "support.sla.retire", "support.sla.acknowledge", "support.evidence.revoke"])
    expect(supportCommandPurchase(operation).purpose).toBe("settle-existing");
  expect(supportCommandPurchase("support.session.issue-capability").capability).toBe("support.tickets");
  expect(supportCommandPurchase("support.evidence.bind").capability).toBe("support.evidence");
});
