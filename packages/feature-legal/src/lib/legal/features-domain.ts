import { z } from "zod";
import { dateSchema, type LegalKind } from "@erpx/server-legal/public/rules/model";
export type FeatureField = {
  key: string;
  label: string;
  type?: "date" | "number" | "textarea";
  required?: boolean;
};
export type FeatureSpec = {
  label: string;
  kinds: LegalKind[];
  table: string;
  fields: FeatureField[];
  states: Record<string, string[]>;
  initial: string;
  evidenceStates: string[];
  independentStates: string[];
};
const f = (
  key: string,
  label: string,
  type: FeatureField["type"] = "textarea",
  required = true,
): FeatureField => ({ key, label, type, required });
export const featureSpecs = {
  case_event: {
    label: "Hearings, filings and case actions",
    kinds: ["matter"],
    table: "legal_case_events",
    fields: [
      f(
        "eventType",
        "Hearing / filing / order / appeal / settlement / enforcement",
      ),
      f("authority", "Court or authority"),
      f("details", "Action and requirements"),
      f("evidence", "Order, filing receipt or outcome", "textarea", false),
    ],
    initial: "planned",
    states: {
      planned: ["in_progress", "cancelled"],
      in_progress: ["review", "completed", "cancelled"],
      review: ["approved", "in_progress", "rejected"],
      approved: ["completed"],
      completed: [],
      cancelled: [],
      rejected: ["planned"],
    },
    evidenceStates: ["review", "completed"],
    independentStates: ["approved"],
  },
  compliance_action: {
    label: "Controls, findings and remediation",
    kinds: ["compliance"],
    table: "legal_compliance_actions",
    fields: [
      f("control", "Requirement and control"),
      f("assessment", "Applicability, assessment and finding"),
      f("remediation", "Corrective action", "textarea", false),
      f("evidence", "Audit or remediation evidence", "textarea", false),
    ],
    initial: "open",
    states: {
      open: ["remediation", "review", "not_applicable"],
      remediation: ["review"],
      review: ["verified", "remediation"],
      verified: ["closed"],
      not_applicable: ["open"],
      closed: ["open"],
    },
    evidenceStates: ["review", "verified", "not_applicable"],
    independentStates: ["verified"],
  },
  privacy_case: {
    label: "Data-subject requests and breaches",
    kinds: ["privacy"],
    table: "legal_privacy_cases",
    fields: [
      f("caseType", "subject_request / breach / processing"),
      f("basis", "Purpose, legal basis and applicable rule"),
      f("scope", "Data, subjects, systems and recipients"),
      f(
        "assessment",
        "Identity verification or impact assessment",
        "textarea",
        false,
      ),
      f(
        "response",
        "Response, notification and remediation",
        "textarea",
        false,
      ),
      f("evidence", "Response / notification receipt", "textarea", false),
      f("knownAt", "Known event time (ISO 8601 with timezone)", "textarea", false),
      f("riskLevel", "Risk: low / medium / high", "textarea", false),
      f("notificationDecision", "Notification: required / not_required / pending", "textarea", false),
      f("notificationBasis", "Notification decision basis", "textarea", false),
      f("identityVerification", "Identity: verified / missing_information / not_verified", "textarea", false),
      f("missingInformation", "Information still required", "textarea", false),
      f("thirdPartyData", "Third-party data: none / present / unknown", "textarea", false),
      f("thirdPartyHandling", "Third-party handling: redacted / separate_consent / restricted_review", "textarea", false),
      f("responseDecision", "Response: provide / partial / refuse / notify / no_notification", "textarea", false),
      f("refusalReason", "Reason for refusing or limiting the response", "textarea", false),
    ],
    initial: "received",
    states: {
      received: ["awaiting_information", "assessed", "rejected"],
      awaiting_information: ["assessed", "rejected"],
      assessed: ["awaiting_information", "in_progress", "review"],
      in_progress: ["review"],
      review: ["approved", "in_progress", "awaiting_information"],
      approved: ["responded"],
      responded: ["closed"],
      closed: ["received"],
      rejected: [],
    },
    evidenceStates: ["awaiting_information", "assessed", "responded", "closed", "rejected"],
    independentStates: ["approved", "rejected"],
  },
  corporate_action: {
    label: "Corporate filings and permit renewals",
    kinds: ["corporate", "license"],
    table: "legal_corporate_actions",
    fields: [
      f("actionType", "Meeting / resolution / filing / renewal"),
      f("authority", "Authority and submission channel"),
      f("requirements", "Required documents and conditions"),
      f(
        "evidence",
        "Minutes, resolution, receipt or renewed permit",
        "textarea",
        false,
      ),
    ],
    initial: "preparing",
    states: {
      preparing: ["review", "cancelled"],
      review: ["approved", "preparing", "rejected"],
      approved: ["submitted"],
      submitted: ["completed", "preparing"],
      completed: [],
      cancelled: [],
      rejected: ["preparing"],
    },
    evidenceStates: ["review", "submitted", "completed"],
    independentStates: ["approved"],
  },
  ip_action: {
    label: "IP prosecution, renewal and infringement",
    kinds: ["ip"],
    table: "legal_ip_actions",
    fields: [
      f("actionType", "Application / response / renewal / infringement"),
      f("authority", "Jurisdiction and authority"),
      f("requirements", "Classes, required action and grounds"),
      f(
        "evidence",
        "Filing, payment, decision or settlement evidence",
        "textarea",
        false,
      ),
    ],
    initial: "preparing",
    states: {
      preparing: ["review", "cancelled"],
      review: ["approved", "preparing", "rejected"],
      approved: ["filed"],
      filed: ["examined", "completed", "rejected"],
      examined: ["preparing", "completed", "rejected"],
      completed: [],
      cancelled: [],
      rejected: ["preparing"],
    },
    evidenceStates: ["filed", "completed"],
    independentStates: ["approved"],
  },
  counsel_entry: {
    label: "Conflict review, time and fees",
    kinds: ["counsel"],
    table: "legal_counsel_entries",
    fields: [
      f("entryType", "conflict / time / fee / invoice"),
      f("description", "Work, related parties and conflict search"),
      f("minutes", "Minutes", "number", false),
      f("amount", "Amount in minor currency units", "number", false),
      f(
        "evidence",
        "Timesheet, invoice or conflict review evidence",
        "textarea",
        false,
      ),
    ],
    initial: "draft",
    states: {
      draft: ["submitted", "cancelled"],
      submitted: ["approved", "rejected", "draft"],
      approved: ["billed", "closed"],
      billed: ["closed"],
      rejected: ["draft"],
      closed: [],
      cancelled: [],
    },
    evidenceStates: ["submitted", "billed"],
    independentStates: ["approved"],
  },
  schedule: {
    label: "Commitments and scheduled deliverables",
    kinds: ["contract", "obligation", "license", "ip", "matter"],
    table: "legal_obligation_schedule",
    fields: [
      f(
        "scheduleType",
        "Delivery / payment / notice / guarantee / insurance / renewal",
      ),
      f("requirement", "Required action"),
      f("amount", "Amount in minor currency units", "number", false),
      f("evidence", "Completion evidence", "textarea", false),
    ],
    initial: "pending",
    states: {
      pending: ["in_progress", "waiver_review"],
      in_progress: ["completed", "waiver_review"],
      waiver_review: ["waived", "pending"],
      waived: [],
      completed: ["in_progress"],
    },
    evidenceStates: ["completed", "waiver_review"],
    independentStates: ["waived"],
  },
  custody: {
    label: "Evidence custody and transfers",
    kinds: [
      "request",
      "matter",
      "contract",
      "compliance",
      "privacy",
      "corporate",
      "license",
      "ip",
      "counsel",
      "obligation",
    ],
    table: "legal_custody_events",
    fields: [
      f("documentId", "Document version ID"),
      f("from", "Transferor and location"),
      f("to", "Recipient and location"),
      f("purpose", "Purpose and condition"),
      f("evidence", "Receipt and integrity observation", "textarea", false),
    ],
    initial: "prepared",
    states: {
      prepared: ["transferred", "cancelled"],
      transferred: ["received", "disputed"],
      received: [],
      disputed: ["received"],
      cancelled: [],
    },
    evidenceStates: ["transferred", "received", "disputed"],
    independentStates: ["received"],
  },
  policy_ack: {
    label: "Policy acknowledgement",
    kinds: ["compliance", "privacy", "corporate"],
    table: "legal_policy_acknowledgements",
    fields: [
      f("documentId", "Policy document version ID"),
      f("instruction", "Acknowledgement instruction"),
      f("evidence", "Acknowledgement statement", "textarea", false),
    ],
    initial: "pending",
    states: { pending: ["acknowledged"], acknowledged: [] },
    evidenceStates: ["acknowledged"],
    independentStates: [],
  },
  intake: {
    label: "Information requests and responses",
    kinds: ["request"],
    table: "legal_intake_questions",
    fields: [
      f("question", "Information required"),
      f("response", "Response", "textarea", false),
      f("evidence", "Response evidence", "textarea", false),
    ],
    initial: "requested",
    states: {
      requested: ["responded", "cancelled"],
      responded: ["accepted", "requested"],
      accepted: [],
      cancelled: [],
    },
    evidenceStates: ["responded"],
    independentStates: [],
  },
} satisfies Record<string, FeatureSpec>;
export type FeatureKind = keyof typeof featureSpecs;
export type FeatureRow = {
  id: string;
  item_id: string;
  category: FeatureKind;
  title: string;
  owner_id: string;
  due_date: string | null;
  state: string;
  version: number;
  data: Record<string, string>;
  created_by: string;
  updated_by: string;
  updated_at: number;
};
export const featureKinds = Object.keys(featureSpecs) as [
  FeatureKind,
  ...FeatureKind[],
];
const date = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/)
  .refine((s) => {
    const d = new Date(s + "T00:00:00Z");
    return !Number.isNaN(d.valueOf()) && d.toISOString().slice(0, 10) === s;
  }, "Invalid calendar date");
export const featureInput = z.object({
  category: z.enum(featureKinds),
  title: z.string().trim().min(3).max(200),
  ownerId: z.string().min(1),
  dueDate: date.nullable().default(null),
  data: z.record(z.string(), z.string().max(12000)),
});
export function parseFeature(raw: unknown, now:number) {
  const x = featureInput.parse(raw),
    spec: FeatureSpec = featureSpecs[x.category];
  for (const key of Object.keys(x.data))
    if (!spec.fields.some((f) => f.key === key))
      throw new Error(`Unknown field: ${key}`);
  for (const field of spec.fields) {
    const value = x.data[field.key];
    if (field.required && !value?.trim())
      throw new Error(`${field.label} is required`);
    if (value && field.type === "number" && !/^\d{1,14}$/.test(value))
      throw new Error(`${field.label} must be nonnegative whole units`);
  }
  if (x.category === "privacy_case") validatePrivacyCase(x.data,now);
  for (const [field, values] of Object.entries(workflowFieldChoices[x.category] ?? {}))
    if (x.data[field] && !values.includes(x.data[field])) throw new Error(`Choose a valid ${field}.`);
  return x;
}

export const privacyFieldChoices: Record<string, readonly string[]> = {
    caseType: ["subject_request", "breach", "processing"],
    riskLevel: ["low", "medium", "high"],
    notificationDecision: ["required", "not_required", "pending"],
    identityVerification: ["verified", "missing_information", "not_verified"],
    thirdPartyData: ["none", "present", "unknown"],
    thirdPartyHandling: ["redacted", "separate_consent", "restricted_review"],
    responseDecision: ["provide", "partial", "refuse", "notify", "no_notification"],
};
export const workflowFieldChoices: Partial<Record<FeatureKind, Record<string, readonly string[]>>> = {
  case_event: {eventType:["hearing","filing","order","appeal","settlement","enforcement"]},
  corporate_action: {actionType:["meeting","resolution","filing","renewal"]},
  ip_action: {actionType:["application","response","renewal","infringement"]},
  schedule: {scheduleType:["delivery","payment","notice","guarantee","insurance","renewal"]},
  privacy_case: privacyFieldChoices,
  counsel_entry: {entryType:["conflict","time","fee","invoice"]},
};
export function validatePrivacyCase(data: Record<string, string>, now:number) {
  for (const [field, values] of Object.entries(privacyFieldChoices))
    if (data[field] && !values.includes(data[field])) throw new Error(`Choose a valid ${field}.`);
  if (data.knownAt) {
    dateSchema.parse(data.knownAt.slice(0, 10));
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?(?:Z|[+-]\d{2}:\d{2})$/.test(data.knownAt) || !Number.isFinite(Date.parse(data.knownAt)))
      throw new Error("Known event time requires an ISO 8601 timestamp with a timezone.");
    if (Date.parse(data.knownAt) > now) throw new Error("Known event time cannot be in the future.");
  }
  if (data.caseType === "breach" && !data.knownAt) throw new Error("Record when the breach became known, independently of ticket creation.");
}

export function privacyDecisionError(data: Record<string, string>, state: string): string | null {
  if (state === "awaiting_information" && !data.missingInformation?.trim()) return "Record the information still required.";
  if (["assessed", "review", "approved", "responded", "closed"].includes(state)) {
    if (!data.assessment?.trim()) return "Record the identity or impact assessment.";
    if (data.caseType === "subject_request") {
      if (!data.identityVerification || !data.thirdPartyData) return "Record identity verification and whether third-party data is present.";
      if (state !== "assessed" && data.identityVerification !== "verified") return "Obtain the missing identity information before independent response review.";
      if (state !== "assessed" && data.thirdPartyData === "unknown") return "Assess third-party data before independent response review.";
      if (data.thirdPartyData === "present" && state !== "assessed" && !data.thirdPartyHandling) return "Record how third-party data will be protected.";
      if (state !== "assessed" && !["provide", "partial", "refuse"].includes(data.responseDecision)) return "Choose the proposed data-subject response.";
      if (["partial", "refuse"].includes(data.responseDecision) && !data.refusalReason?.trim()) return "Record the reason for limiting or refusing the response.";
    }
    if (data.caseType === "breach" && state !== "assessed") {
      if (!data.riskLevel || !["required", "not_required"].includes(data.notificationDecision) || !data.notificationBasis?.trim()) return "Review the risk and a reasoned notification decision before approval.";
      if (data.responseDecision !== (data.notificationDecision === "required" ? "notify" : "no_notification")) return "The response must match the reviewed notification decision.";
    }
  }
  if (state === "rejected" && (data.responseDecision !== "refuse" || !data.refusalReason?.trim() || !data.response?.trim())) return "A refused request requires a reason and the response to be independently reviewed.";
  if (["responded", "closed"].includes(state) && !data.response?.trim()) return "Retain the response or notification evidence.";
  return null;
}
