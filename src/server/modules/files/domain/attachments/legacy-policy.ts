import type { AttachmentComponentKind } from "../../domain/attachments/types";

export type LegacyAttachmentPolicy = {
  purposeKey: string;
  componentKind: AttachmentComponentKind;
  componentRef: string | null;
  completion: "bind_immediately" | "stage_for_domain";
};

/**
 * One-release compatibility aliases are intentionally exact. Historical or
 * client-supplied purpose values that are not listed here remain quarantined
 * instead of being silently widened to a generic audience or purpose.
 */
export function legacyAttachmentPolicy(
  ownerType: string,
  purpose: string,
): LegacyAttachmentPolicy | null {
  switch (ownerType) {
    case "support_ticket":
      if (purpose === "support_evidence" || purpose === "internal_evidence")
        return { purposeKey: "internal_evidence", componentKind: "action", componentRef: "support-evidence:internal_evidence", completion: "bind_immediately" };
      if (purpose === "customer_shareable")
        return { purposeKey: "customer_shareable", componentKind: "action", componentRef: "support-evidence:customer_shareable", completion: "bind_immediately" };
      return null;
    case "employee_request":
      return ["request_evidence", "it_request_evidence", "fulfillment"].includes(purpose)
        ? { purposeKey: purpose, componentKind: "action", componentRef: "legacy-upload-adapter", completion: "bind_immediately" }
        : null;
    case "leave_request":
      if (purpose === "medical_certificate")
        return { purposeKey: "medical_certificate", componentKind: "record", componentRef: "leave-evidence:medical_certificate", completion: "stage_for_domain" };
      if (purpose === "leave_supporting_photo" || purpose === "evidence")
        return { purposeKey: "leave_supporting_photo", componentKind: "record", componentRef: "leave-evidence:leave_supporting_photo", completion: "stage_for_domain" };
      return null;
    case "expense_claim":
      return ["receipt", "merchant_receipt", "supporting_document"].includes(purpose)
        ? { purposeKey: purpose, componentKind: "record", componentRef: "legacy-upload-adapter", completion: "stage_for_domain" }
        : null;
    case "expense_payment":
      return purpose === "payment_slip"
        ? { purposeKey: "payment_slip", componentKind: "action", componentRef: "legacy-upload-adapter", completion: "stage_for_domain" }
        : null;
    case "invoice_payment":
      return purpose === "payment_slip" || purpose === "bank_payment_slip"
        ? { purposeKey: "payment_slip", componentKind: "action", componentRef: "legacy-upload-adapter", completion: "stage_for_domain" }
        : null;
    case "staff_task":
      return purpose === "task_evidence" || purpose === "evidence"
        ? { purposeKey: "task_evidence", componentKind: "action", componentRef: null, completion: "stage_for_domain" }
        : null;
    case "social_draft":
      return purpose === "post_image" || purpose === "post-image"
        ? { purposeKey: "post_image", componentKind: "record", componentRef: "legacy-upload-adapter", completion: "bind_immediately" }
        : null;
    case "quality_hold":
      return purpose === "quality_inspection_evidence"
        ? { purposeKey: "quality_inspection_evidence", componentKind: "action", componentRef: "inspection", completion: "stage_for_domain" }
        : null;
    case "commerce_order_delivery":
      return purpose === "proof_of_delivery"
        ? { purposeKey: "proof_of_delivery", componentKind: "action", componentRef: "delivery", completion: "stage_for_domain" }
        : null;
    case "holding_brand":
    case "branch_brand":
      return purpose === "primary" || purpose === "inverse"
        ? { purposeKey: purpose, componentKind: "publication", componentRef: "legacy-upload-adapter", completion: "bind_immediately" }
        : null;
    case "company_storefront_brand":
      return purpose === "logo"
        ? { purposeKey: "logo", componentKind: "publication", componentRef: "legacy-upload-adapter", completion: "bind_immediately" }
        : null;
    default:
      return null;
  }
}
