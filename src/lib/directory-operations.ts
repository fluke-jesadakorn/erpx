import { z } from "zod";

const id = z.string().min(1).max(500);
const text = z.string().trim().min(1).max(500);
const unc = z
  .string()
  .regex(/^\\\\[^\\\s]+\\[^\r\n]+$/, "Use a UNC share path.")
  .max(500);
export const windowsPolicyDefinition = z
  .object({
    name: z.string().trim().min(2).max(120),
    scope: z.enum(["user", "computer"]),
    targetOuId: id,
    filterGroupId: id,
    computerTargetId: id.nullable().default(null),
    usb: z.enum(["unchanged", "deny", "allow"]).default("unchanged"),
    lockSeconds: z.number().int().min(60).max(86400).nullable().default(null),
    wallpaper: z.string().max(500).nullable().default(null),
    wallpaperStyle: z.enum(["0", "2", "6", "10"]).default("10"),
    drives: z
      .array(
        z.object({
          id: z.string().uuid(),
          letter: z.string().regex(/^[D-Z]$/),
          path: unc,
          label: text,
          remove: z.boolean().default(false),
        }),
      )
      .max(20)
      .default([]),
    printers: z
      .array(
        z.object({
          id: z.string().uuid(),
          path: unc,
          isDefault: z.boolean().default(false),
          remove: z.boolean().default(false),
        }),
      )
      .max(20)
      .default([]),
  })
  .superRefine((v, c) => {
    if (v.scope === "computer" && !v.computerTargetId)
      c.addIssue({
        code: "custom",
        message: "Choose an explicit computer OU or group.",
      });
    if (
      v.scope === "computer" &&
      (v.wallpaper || v.drives.length || v.printers.length)
    )
      c.addIssue({
        code: "custom",
        message:
          "Desktop, Drive and Printer preferences require a user policy.",
      });
    if (
      v.wallpaper &&
      !/^(?:[A-Za-z]:\\|\\\\)[^\r\n]+\.(bmp|jpe?g)$/i.test(v.wallpaper)
    )
      c.addIssue({
        code: "custom",
        message: "Choose a local or UNC BMP/JPEG wallpaper path.",
      });
    if (new Set(v.drives.map((d) => d.letter)).size !== v.drives.length)
      c.addIssue({
        code: "custom",
        message: "Each drive letter must be unique.",
      });
    if (v.printers.filter((p) => p.isDefault && !p.remove).length > 1)
      c.addIssue({
        code: "custom",
        message: "Choose only one default printer.",
      });
  });
export type WindowsPolicyDefinition = z.infer<typeof windowsPolicyDefinition>;
export const directoryAction = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("account.create"),
    username: z.string().regex(/^[a-zA-Z][a-zA-Z0-9._-]{1,63}$/),
    firstName: text,
    lastName: text,
    parentId: id,
  }),
  z.object({
    kind: z.literal("account.update"),
    objectId: id,
    fields: z.record(z.string(), z.union([z.string().max(2000), z.boolean()])),
  }),
  z.object({
    kind: z.literal("account.disable"),
    objectId: id,
    caseId: id.nullable().default(null),
  }),
  z.object({ kind: z.literal("account.enable"), objectId: id, caseId: id }),
  z.object({
    kind: z.literal("group.create"),
    name: z.string().regex(/^[a-zA-Z0-9][a-zA-Z0-9 ._-]{1,99}$/),
    parentId: id,
  }),
  z.object({
    kind: z.literal("group.members"),
    objectId: id,
    memberIds: z.array(id).min(1).max(100),
    action: z.enum(["add", "remove"]),
  }),
  z.object({
    kind: z.literal("ou.create"),
    name: z.string().regex(/^[a-zA-Z0-9][a-zA-Z0-9 ._-]{1,99}$/),
    parentId: id,
  }),
  z.object({
    kind: z.literal("ou.update"),
    objectId: id,
    description: z.string().max(500),
  }),
  z.object({ kind: z.literal("object.move"), objectId: id, parentId: id }),
  z.object({
    kind: z.literal("gpo.publish"),
    objectId: id.nullable().default(null),
    definition: windowsPolicyDefinition,
  }),
  z.object({
    kind: z.literal("gpo.link"),
    objectId: id,
    parentId: id,
    filterGroupId: id.optional(),
    enabled: z.boolean().default(true),
    enforced: z.boolean().default(false),
    order: z.number().int().min(1).max(1000).optional(),
    remove: z.boolean().default(false),
  }),
  z.object({
    kind: z.literal("gpo.order"),
    objectId: id,
    gpoIds: z.array(id).max(1000),
  }),
  z.object({
    kind: z.literal("gpo.inheritance"),
    objectId: id,
    blocked: z.boolean(),
  }),
  z.object({
    kind: z.literal("gpo.filter"),
    objectId: id,
    groupIds: z.array(id).min(1).max(100),
  }),
]);
export type DirectoryAction = z.infer<typeof directoryAction>;
export const directoryChangeInput = z.object({
  connectionId: id,
  expectedVersion: z.number().int().positive(),
  action: directoryAction,
  additionalActions: z.array(directoryAction).max(19).default([]),
  reason: z.string().trim().min(3).max(1000),
  affectedCompanyIds: z.array(id).max(100).default([]),
});
export type DirectoryChangeInput = z.infer<typeof directoryChangeInput>;
export type DirectoryObject = {
  id: string;
  dn: string;
  name: string;
  kind: "account" | "group" | "ou" | "computer" | "gpo";
  parentId: string | null;
  version: string;
  module: string;
  properties: Record<string, unknown>;
  memberIds: string[];
};
export type ConnectorField = {
  name: string;
  type: "text" | "checkbox";
  read: boolean;
  write: boolean;
};
export type DirectoryInventory = {
  objects: DirectoryObject[];
  fields: ConnectorField[];
  providerVersion: string;
  observedAt: number;
  complete: true;
};
export type DirectoryJob = {
  id: string;
  connection_id: string;
  status: string;
  version: number;
  definition: DirectoryChangeInput;
  last_error: string | null;
  result: Record<string, unknown> | null;
  review_id: string | null;
  approval_mode?: "direct" | "reviewed";
  steps?: { index: number; status: string; result?: Record<string, unknown> }[];
  created_at: number;
};
export type DirectoryAutomationData = {
  cases: {
    id: string;
    name: string;
    kind: string;
    effectiveAt: number;
    accountIds: string[];
  }[];
  connections: {
    id: string;
    name: string;
    provider: string;
    version: number;
    writeEnabled: boolean;
    testedVersion: number | null;
    inventory: DirectoryInventory | null;
    mappings: {
      attributeId: string;
      field: string;
      direction: "read" | "write";
    }[];
  }[];
  jobs: DirectoryJob[];
  canApply?: boolean;
  canManage: boolean;
  canReview: boolean;
};
export type DirectoryPreview = {
  token: string;
  objects: string[];
  companies: { id: string; name: string }[];
  people: string[];
  computers: string[];
  positions: string[];
  warnings: string[];
};

export type RegistryPolicy = {
  key: string;
  name: string;
  type: "DWord" | "String";
  value: string | number;
};
/** A finite editor, never a registry path or PowerShell supplied by the browser. */
export function compileAdministrativePolicies(
  p: WindowsPolicyDefinition,
): RegistryPolicy[] {
  const result: RegistryPolicy[] = [],
    hive = p.scope === "computer" ? "HKLM" : "HKCU";
  if (p.usb !== "unchanged")
    result.push({
      key: `${hive}\\Software\\Policies\\Microsoft\\Windows\\RemovableStorageDevices`,
      name: "Deny_All",
      type: "DWord",
      value: p.usb === "deny" ? 1 : 0,
    });
  if (p.lockSeconds !== null) {
    if (p.scope === "computer")
      result.push({
        key: "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
        name: "InactivityTimeoutSecs",
        type: "DWord",
        value: p.lockSeconds,
      });
    else
      for (const [name, value] of Object.entries({
        ScreenSaveActive: "1",
        ScreenSaverIsSecure: "1",
        ScreenSaveTimeOut: String(p.lockSeconds),
        "SCRNSAVE.EXE": "scrnsave.scr",
      }))
        result.push({
          key: "HKCU\\Software\\Policies\\Microsoft\\Windows\\Control Panel\\Desktop",
          name,
          type: "String",
          value,
        });
  }
  if (p.wallpaper)
    for (const [name, value] of Object.entries({
      Wallpaper: p.wallpaper,
      WallpaperStyle: p.wallpaperStyle,
    }))
      result.push({
        key: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
        name,
        type: "String",
        value,
      });
  return result;
}
