import { z } from "zod";
import { workplaceModuleDestinations } from "../workplace-catalogue";
import type { SourceRecordReference } from "../source-record";

export const assistantPageSchema = z.object({
  path: z.string().min(1).max(1500),
  mode: z.enum(["page", "workspace"]).default("page"),
});
export const assistantAskSchema = assistantPageSchema.extend({
  question: z.string().trim().min(1).max(4000),
  contextPaths: z.array(z.string().min(1).max(1500)).max(5).default([]),
});
export const assistantAnswerSchema = z.object({
  answer: z.string().min(1).max(12000),
  sourceIds: z.array(z.string().max(180)).max(20).default([]),
  followUps: z.array(z.string().max(250)).max(3).default([]),
});
export type AssistantAnswer = z.infer<typeof assistantAnswerSchema>;
export type PageGuide = { title: string; purpose: string; steps: string[]; prompts: string[] };
export const pageGuides: Record<string, PageGuide> = {
  home: { title: "Today", purpose: "Understand team activity, share updates and hand over linked work.", steps: ["Read Latest updates or Following for team conversations.", "Use Start to begin work or Calendar to plan the day.", "Open My work for your actions, approvals and tracked requests."], prompts: ["Summarize my team's recent updates.", "Explain how to start work."] },
  work: { title: "My work", purpose: "Act, decide and track work through its responsible owner.", steps: ["Use My actions for steps you can perform and Approvals for permitted decisions.", "Follow My requests and Waiting under Tracking.", "Open linked work for evidence, the current owner and next step."], prompts: ["Summarize this queue and its blockers.", "What information should I review before deciding?"] },
  requests: { title: "Request", purpose: "Make the need clear and follow approval and fulfilment separately.", steps: ["Save a clear description of the need and location.", "Use AI preparation to review proposed wording and category.", "Review the responsible queue and next action before submitting."], prompts: ["Summarize this request and what is missing.", "Explain the next step."] },
  expenses: { title: "Expense", purpose: "Turn supporting documents into a reviewed expense draft.", steps: ["Attach the receipt or invoice to this draft.", "Use document reading, then compare proposed fields with the original evidence.", "Check amounts, currency, tax and duplicates before submitting for approval."], prompts: ["Check whether this expense is complete.", "Explain how to review receipt extraction."] },
  commerce: { title: "Sales & purchasing", purpose: "Follow an order from preparation through fulfilment and accounting.", steps: ["Choose the correct order type and counterparty.", "Check products, quantities, prices and delivery details.", "Review approvals, then record fulfilment before invoicing."], prompts: ["Explain the workflow on this page.", "What should I check before confirming?"] },
  finance: { title: "Finance", purpose: "Review accounting evidence before posting or settling transactions.", steps: ["Choose the accounting period and document type.", "Reconcile amounts, currency and source evidence.", "Review the approval and posting status; payment is a separate action."], prompts: ["Explain what I should check on this page.", "Summarize the status from the supplied information."] },
  people: { title: "People", purpose: "Keep a person's employment record and account access consistent.", steps: ["Locate the person and the correct company engagement.", "Review effective dates, position and reporting line.", "Review access changes through the applicable approval process."], prompts: ["Explain the difference between an account and an employee.", "What should I check when an employee changes teams?"] },
  calendar: { title: "Calendar", purpose: "Understand work commitments in the selected date range and timezone.", steps: ["Choose your date range, team scope and timezone.", "Select the event layers needed for this task.", "Open an event to use its owning workflow; a calendar entry is not an approval."], prompts: ["Explain how to choose calendar layers.", "Why are some event details unavailable?"] },
  "staff-tasks": { title: "Staff task", purpose: "Complete assigned work with clear evidence and handover.", steps: ["Read the task, owner, due date and blocked reason.", "Record progress and attach evidence.", "Submit completion for review when required."], prompts: ["Summarize this task and its next steps.", "What evidence should I prepare?"] },
  projects: { title: "Projects", purpose: "Coordinate related work, ownership and progress.", steps: ["Open the project and confirm its scope.", "Review tasks, owners and dependencies.", "Update the owning task with progress and blockers."], prompts: ["Explain how to track project progress.", "How should I handle blocked work?"] },
  handbook: { title: "Company handbook", purpose: "Use published company guidance and keep drafts under review.", steps: ["Search for the relevant policy or procedure.", "Check the published revision and its applicability.", "Editors prepare a new revision before publishing changes."], prompts: ["Explain the procedure in this handbook.", "Summarize the actions required by this article."] },
  learn: { title: "Guides", purpose: "Learn the workflow you are working in and consult approved guidance.", steps: ["Choose a workflow you want to learn.", "Open that page and ask the assistant to explain its steps.", "Use published handbook articles for company-specific policy."], prompts: ["Teach me how to get started with ERPX.", "Which preparation tasks can AI help with?"] },
  pos: { title: "Point of sale", purpose: "Prepare a sale and complete payment and fulfilment using the POS workflow.", steps: ["Confirm the location, session and selected products.", "Review quantity, stock and order total.", "Complete payment and fulfilment using the available POS actions."], prompts: ["Explain sales and fulfillment.", "What should I check when a payment fails?"] },
  pms: { title: "Hotel operations", purpose: "Keep reservations, stays, rooms and guest charges aligned.", steps: ["Select the property and the correct reservation or stay.", "Check dates, room availability and guest folio.", "Use the owning PMS action for check-in, charges and check-out."], prompts: ["Explain the check-in workflow.", "What should I check in the guest folio?"] },
  inbox: { title: "Inbox", purpose: "Read work conversations and follow up in the related workflow.", steps: ["Choose the relevant conversation.", "Review its participants and linked work.", "Open linked work to take the business action."], prompts: ["Explain the difference between a chat and an approval."] },
  leave: { title: "Leave", purpose: "Prepare a leave request against current eligibility and entitlement.", steps: ["Check the leave type, dates and available entitlement.", "Review the requested duration and required evidence.", "Submit and follow the approval status."], prompts: ["What should I prepare for a leave request?"] },
  overtime: { title: "Overtime", purpose: "Prepare overtime hours and evidence for review.", steps: ["Check eligibility and the actual work period.", "Provide the reason and supporting evidence.", "Review hours before submission; payroll uses approved records."], prompts: ["Explain how to request overtime."] },
  settings: { title: "Settings", purpose: "Review and test a configuration before activating it.", steps: ["Select the configuration area.", "Review the current active settings and any draft changes.", "Test and activate only the reviewed revision."], prompts: ["Explain the AI settings.", "Why should I test before activation?"] },
};

// Every discoverable owner area has a manual guide, even before a read adapter exists.
for (const [moduleId, destination] of Object.entries(workplaceModuleDestinations)) {
  const root = destination.path.split(/[/?]/)[0];
  if (pageGuides[root]) continue;
  pageGuides[root] = { title: moduleId.replaceAll("_", " ").replace(/^./,letter=>letter.toUpperCase()), purpose: destination.objective,
    steps: ["Open the source record and check its current status and responsible person.", "Review evidence and the permitted next step in the owning workspace.", "Confirm your action there, then follow progress in My work."], prompts: ["Explain this workflow.", "What should I review before continuing?"] };
}
for (const [root, title, owner] of [["customer-followups","Follow-ups","crm"],["customer-duplicates","Duplicate reviews","customer_authority"],["customer-access","Customer access reviews","customer_authority"],
  ["quality","Quality","quality"],["stock-transit","Stock in transit","inventory"],["stock-counts","Stock counts","inventory"],["stock-transfers","Transfers","inventory"],
  ["procurement-rfqs","Supplier quotations","procurement"],["supplier-bills","Invoice matching","procurement"],["supplier-promises","Delivery promises","procurement"],
  ["routine-templates","Routine templates","operations"],["routine-schedules","Routine schedules","operations"],["executive","Insights","finance"],["posts","Conversation","collaboration"]]) {
  pageGuides[root] = { ...pageGuides[workplaceModuleDestinations[owner].path.split(/[/?]/)[0]], title };
}

export function assistantSupport(path: string) {
  const { segments } = assistantPage(path), [root,id] = segments;
  return ["requests","expenses","staff-tasks","leave","overtime"].includes(root) ? "prepare"
    : ["home","posts","work","pms","pos","finance"].includes(root) || root === "handbook" && id && id !== "new" || root === "commerce" && path.includes("view=stock") ? "read" : "guide";
}

/** URL parameters define context; navigation-only pagination/sorting do not join histories. */
export function assistantPage(path: string) {
  if (!path.startsWith("/") || path.startsWith("//") || path.includes("\\") || path.split("?")[0].split("/").some(s=>s==="."||s===".."||s.includes("%"))) throw Error("Invalid assistant page");
  const url = new URL(path, "https://erpx.invalid");
  const segments = url.pathname.split("/").filter(Boolean);
  if (!segments.length || !pageGuides[segments[0]] || segments.some(p => !/^[a-zA-Z0-9_.-]+$/.test(p))) throw Error("Unsupported assistant page");
  const query = new URLSearchParams();
  for (const key of [...new Set(url.searchParams.keys())].sort()) {
    if (["cursor", "limit", "sort", "return", "frontReturn", "offset", "movementCursor"].includes(key)) continue;
    for (const value of url.searchParams.getAll(key).sort()) query.append(key, value.slice(0,300));
  }
  return { path: `/${segments.join("/")}${query.size ? `?${query}` : ""}`, segments, query, guide: pageGuides[segments[0]] };
}

export type AssistantSource = { id: string; title: string; path: string; version: string; source?: SourceRecordReference; data: unknown };
export type AssistantTurn = { id: string; question: string; answer: AssistantAnswer; sources: Omit<AssistantSource,"data">[]; createdAt: number };
export type AssistantView = { page: string; guide: PageGuide; crossContext: boolean; unavailable: string | null; turns: AssistantTurn[]; hiddenTurns: number };
