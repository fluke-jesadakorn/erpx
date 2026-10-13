import { z } from "zod";

export const intakeIntentSchema = z.enum(["purchase", "expense", "leave", "task", "post", "unknown"]);
export type IntakeIntent = z.infer<typeof intakeIntentSchema>;

export const guidedIntakeInputSchema = z.object({
  message: z.string().trim().min(3).max(4000),
  contextPath: z.string().min(1).max(1500).default("/home"),
});

/** The model may propose text, but never routes, permissions, policy, or commands. */
export const guidedIntakeModelSchema = z.object({
  intent: intakeIntentSchema,
  summary: z.string().trim().min(1).max(600),
  draft: z.record(z.string(), z.string().max(3000)).default({}),
  sourceIds: z.array(z.string().max(180)).max(10).default([]),
});

export const intakeFields: Record<Exclude<IntakeIntent, "unknown">, readonly string[]> = {
  purchase: ["title", "description", "itemName", "quantity", "location", "neededDate"],
  expense: ["paymentIntent", "merchant", "businessPurpose", "expenseDate", "amount", "currency"],
  leave: ["leaveType", "startDate", "endDate", "reason"],
  task: ["title", "description", "assignee", "dueDate"],
  post: ["body", "purpose"],
};

export function intakeMissing(intent: IntakeIntent, draft: Record<string,string>) {
  if (intent === "unknown") return ["Choose what you would like to do"];
  const required: Record<Exclude<IntakeIntent,"unknown">, [string,string][]> = {
    purchase: [["itemName","What item or service is needed"],["quantity","Quantity"],["description","Reason or business need"]],
    expense: [["paymentIntent","Whether you paid or the company should pay"],["businessPurpose","Business purpose"],["amount","Amount"],["expenseDate","Date of expense"]],
    leave: [["leaveType","Leave type"],["startDate","Start date"],["endDate","End date"]],
    task: [["title","Task title"]],
    post: [["body","Update text"]],
  };
  const result = required[intent].filter(([key]) => !draft[key]).map(([,label]) => label);
  if (intent === "expense") result.push("Receipt or payment evidence");
  if (intent === "leave") result.push("Eligibility and balance check in the leave form");
  if (intent === "post") result.push("Audience selection before publishing");
  return result;
}

export type GuidedIntakeAction = {
  id: Exclude<IntakeIntent, "unknown">;
  label: string;
  href: string;
  suggested: boolean;
};

export type GuidedIntakeResult = {
  intent: IntakeIntent;
  summary: string;
  draft: Record<string, string>;
  missing: string[];
  sources: { id: string; title: string; path: string | null; version: string }[];
  actions: GuidedIntakeAction[];
};

