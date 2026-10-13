import { z } from "zod";
import { attributeInput } from "@/lib/identity_access/user-management";
export const holdingTemplateDefinition = z
  .object({
    onboardingTasks: z.array(z.string().trim().min(2).max(300)).max(50),
    offboardingTasks: z.array(z.string().trim().min(2).max(300)).max(50),
    attributes: z
      .array(
        z
          .object(attributeInput.shape)
          .omit({ id: true, expectedVersion: true, reason: true })
          .extend({ key: z.string().regex(/^[a-z][a-z0-9_-]{1,60}$/) }),
      )
      .max(50),
  })
  .refine(
    (v) => new Set(v.attributes.map((a) => a.key)).size === v.attributes.length,
    "Use each stable field key once.",
  )
  .superRefine((v, c) => {
    for (const [index, a] of v.attributes.entries()) {
      const parsed = attributeInput.safeParse({
        ...a,
        expectedVersion: 0,
        reason: "Holding template field",
      });
      if (!parsed.success)
        c.addIssue({
          code: "custom",
          path: ["attributes", index],
          message: "Provide valid field options and access rules.",
        });
      if (
        a.readRoles?.length &&
        a.writeRoles?.some((r) => !a.readRoles!.includes(r))
      )
        c.addIssue({
          code: "custom",
          path: ["attributes", index],
          message: "Every field editor must also be able to view it.",
        });
    }
  });
export const holdingTemplateInput = z.object({
  id: z.string().optional(),
  expectedVersion: z.number().int().nonnegative(),
  name: z.string().trim().min(2).max(120),
  definition: holdingTemplateDefinition,
  reason: z.string().trim().min(3).max(1000),
});
export type HoldingPeopleTemplate = {
  id: string;
  name: string;
  version: number;
  definition: z.infer<typeof holdingTemplateDefinition>;
  adoptedVersion: number | null;
};
export type PeopleTemplatesData = {
  templates: HoldingPeopleTemplate[];
  canPublish: boolean;
  canAdopt: boolean;
};
