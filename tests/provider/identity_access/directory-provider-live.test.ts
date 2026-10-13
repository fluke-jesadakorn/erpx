import { expect, it, vi } from "vitest";
import { resolve } from "node:path";
import {
  readAutomationInventory,
  applyDirectoryAction,
} from "../../../src/server/modules/identity_access/public/directory-provider";
import type { ProviderConnection } from "../../../src/server/modules/identity_access/public/iam-providers";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  runtimeEnv: async () => ({
    ERPX_IAM_SECRET_PROFILES: resolve(
      process.env.ERPX_UCS_PROFILE_FILE ?? ".scratch/ucs-lab/profiles.json",
    ),
  }),
}));
const connection: ProviderConnection = {
  id: "erpx-ucs-lab",
  provider: "univention",
  config: {
    mode: "ldaps",
    authority: "erpx.test",
    secretProfile: "erpx-ucs-lab",
    baseDn: "ou=ERPX-Lab,dc=erpx,dc=test",
    ldapUrl: "ldaps://127.0.0.1:18636",
  },
};
it.skipIf(process.env.ERPX_VERIFY_UCS_LAB !== "1")(
  "verifies real UCS create, retry, update, membership, conflict, enable, move retry and disable read-back",
  async () => {
    const before = await readAutomationInventory(connection),
      parent = before.objects.find(
        (o) => o.dn.toLowerCase() === "ou=erpx-lab,dc=erpx,dc=test",
      )!;
    expect(parent).toBeTruthy();
    expect(before.fields.map((f) => f.name)).toContain("firstname");
    const suffix = Date.now().toString(36),
      userOperation = crypto.randomUUID();
    const create = {
      kind: "account.create" as const,
      username: "erpx-" + suffix,
      firstName: "ERPX",
      lastName: "Verification",
      parentId: parent.id,
    };
    const user = await applyDirectoryAction(
      connection,
      userOperation,
      create,
      before,
    );
    const replay = await applyDirectoryAction(
      connection,
      userOperation,
      create,
      before,
    );
    expect(replay.objectId).toBe(user.objectId);
    const group = await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "group.create",
        name: "ERPX-Group-" + suffix,
        parentId: parent.id,
      },
      before,
    );
    const snapshot = await readAutomationInventory(connection);
    const account = snapshot.objects.find((o) => o.id === user.objectId)!;
    expect(account.properties.disabled).toBe(true);
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "account.update",
        objectId: account.id,
        fields: { firstname: "Verified" },
      },
      snapshot,
    );
    await expect(
      applyDirectoryAction(
        connection,
        crypto.randomUUID(),
        {
          kind: "account.update",
          objectId: account.id,
          fields: { firstname: "Stale" },
        },
        snapshot,
      ),
    ).rejects.toMatchObject({ code: "DIRECTORY_VERSION_CHANGED" });
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "group.members",
        objectId: String(group.objectId),
        memberIds: [account.id],
        action: "add",
      },
      snapshot,
    );
    const latest = await readAutomationInventory(connection);
    expect(
      latest.objects.find((o) => o.id === group.objectId)?.memberIds,
    ).toContain(account.id);
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "account.enable",
        objectId: account.id,
        caseId: "provider-test-only",
      },
      latest,
    );
    const enabled = await readAutomationInventory(connection);
    expect(
      enabled.objects.find((o) => o.id === account.id)?.properties.disabled,
    ).toBe(false);
    const destination = enabled.objects.find(
      (o) => o.dn.toLowerCase() === "ou=moved,ou=erpx-lab,dc=erpx,dc=test",
    )!;
    expect(destination).toBeTruthy();
    const move = {
        kind: "object.move" as const,
        objectId: account.id,
        parentId: destination.id,
      },
      operation = crypto.randomUUID();
    const moved = await applyDirectoryAction(
      connection,
      operation,
      move,
      enabled,
    );
    expect(
      (await applyDirectoryAction(connection, operation, move, enabled)).dn,
    ).toBe(moved.dn);
    const final = await readAutomationInventory(connection);
    expect(final.objects.find((o) => o.id === account.id)?.parentId).toBe(
      destination.id,
    );
    const disabled = await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      { kind: "account.disable", objectId: account.id, caseId: null },
      final,
    );
    expect(disabled.verified).toBe(true);
    const ou = await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "ou.create",
        name: "Verify-" + suffix,
        parentId: parent.id,
      },
      await readAutomationInventory(connection),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "ou.update",
        objectId: String(ou.objectId),
        description: "ERPX isolated verification",
      },
      await readAutomationInventory(connection),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "object.move",
        objectId: String(group.objectId),
        parentId: String(ou.objectId),
      },
      await readAutomationInventory(connection),
    );
    await expect(
      applyDirectoryAction(
        connection,
        crypto.randomUUID(),
        {
          kind: "object.move",
          objectId: String(ou.objectId),
          parentId: destination.id,
        },
        await readAutomationInventory(connection),
      ),
    ).rejects.toMatchObject({ code: "DIRECTORY_CONTAINER_NOT_EMPTY" });
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "object.move",
        objectId: String(group.objectId),
        parentId: parent.id,
      },
      await readAutomationInventory(connection),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "object.move",
        objectId: String(ou.objectId),
        parentId: destination.id,
      },
      await readAutomationInventory(connection),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "object.move",
        objectId: String(group.objectId),
        parentId: String(ou.objectId),
      },
      await readAutomationInventory(connection),
    );
    const outer = await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "group.create",
        name: "ERPX-Outer-" + suffix,
        parentId: parent.id,
      },
      await readAutomationInventory(connection),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "group.members",
        objectId: String(outer.objectId),
        memberIds: [String(group.objectId)],
        action: "add",
      },
      await readAutomationInventory(connection),
    );
    const verified = await readAutomationInventory(connection);
    expect(verified.objects.find((o) => o.id === ou.objectId)?.parentId).toBe(
      destination.id,
    );
    expect(
      verified.objects.find((o) => o.id === group.objectId)?.parentId,
    ).toBe(ou.objectId);
    expect(
      verified.objects.find((o) => o.id === outer.objectId)?.memberIds,
    ).toContain(group.objectId);
  },
  600000,
);

it.skipIf(process.env.ERPX_VERIFY_GPO_LAB !== "1")(
  "writes GPO settings, links, filtering, order and inheritance through verified UCS HTTPS",
  async () => {
    const inventory = () => readAutomationInventory(connection);
    const before = await inventory();
    const ou = before.objects.find(
      (o) => o.dn.toLowerCase() === "ou=erpx-lab,dc=erpx,dc=test",
    )!;
    const group = before.objects.find((o) => o.name === "ERPX-TestGroup")!;
    expect(ou.properties.windowsIdentity).toBeTruthy();
    const recipe = {
      id: crypto.randomUUID(),
      name: "ERPX-GPO-HTTPS-" + Date.now().toString(36),
      scope: "user" as const,
      targetOuId: ou.id,
      filterGroupId: group.id,
      computerTargetId: null,
      usb: "deny" as const,
      lockSeconds: 600,
      wallpaper: null,
      wallpaperStyle: "10" as const,
      drives: [
        {
          id: crypto.randomUUID(),
          letter: "V",
          label: "Verification",
          path: "\\\\ucs.erpx.test\\lab",
          remove: false,
        },
      ],
      printers: [
        {
          id: crypto.randomUUID(),
          path: "\\\\ucs.erpx.test\\LabPrinter",
          isDefault: false,
          remove: false,
        },
      ],
    };
    const operation = crypto.randomUUID(),
      action = {
        kind: "gpo.publish" as const,
        objectId: null,
        definition: recipe,
      };
    const created = await applyDirectoryAction(
      connection,
      operation,
      action,
      before,
    );
    expect(created).toMatchObject({ verified: true, endpointConfirmed: false });
    expect(
      await applyDirectoryAction(connection, operation, action, before),
    ).toEqual(created);
    const id = String(created.objectId),
      first = await inventory();
    expect(first.objects.find((o) => o.id === id)?.memberIds).toContain(ou.id);
    const update = {
      kind: "gpo.publish" as const,
      objectId: id,
      definition: { ...recipe, lockSeconds: 900 },
    };
    expect(
      (
        await applyDirectoryAction(
          connection,
          crypto.randomUUID(),
          update,
          first,
        )
      ).verified,
    ).toBe(true);
    await expect(
      applyDirectoryAction(connection, crypto.randomUUID(), update, first),
    ).rejects.toMatchObject({ code: "DIRECTORY_VERSION_CHANGED" });
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "gpo.link",
        objectId: id,
        parentId: ou.id,
        enabled: false,
        enforced: true,
        remove: false,
        order: 1,
      },
      await inventory(),
    );
    const linked = await inventory();
    expect(
      (
        linked.objects.find((o) => o.id === ou.id)!.properties.gpoLinks as {
          gpoId: string;
          enabled: boolean;
          enforced: boolean;
        }[]
      ).find((l) => l.gpoId === id),
    ).toMatchObject({ enabled: false, enforced: true });
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      { kind: "gpo.filter", objectId: id, groupIds: [group.id] },
      linked,
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      { kind: "gpo.inheritance", objectId: ou.id, blocked: true },
      await inventory(),
    );
    const blocked = await inventory();
    expect(
      blocked.objects.find((o) => o.id === ou.id)!.properties.blockInheritance,
    ).toBe(true);
    const links = (
      blocked.objects.find((o) => o.id === ou.id)!.properties.gpoLinks as {
        gpoId: string;
      }[]
    )
      .map((l) => l.gpoId)
      .reverse();
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      { kind: "gpo.order", objectId: ou.id, gpoIds: links },
      blocked,
    );
    expect(
      (await inventory()).objects.find((o) => o.id === ou.id)!.properties
        .gpoLinks,
    ).toEqual(links.map((gpoId) => expect.objectContaining({ gpoId })));
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      { kind: "gpo.inheritance", objectId: ou.id, blocked: false },
      await inventory(),
    );
    await applyDirectoryAction(
      connection,
      crypto.randomUUID(),
      {
        kind: "gpo.link",
        objectId: id,
        parentId: ou.id,
        remove: true,
        enabled: false,
        enforced: false,
      },
      await inventory(),
    );
  },
  240000,
);
