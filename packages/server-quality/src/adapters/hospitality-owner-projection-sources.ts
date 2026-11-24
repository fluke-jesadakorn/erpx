import "server-only";
import { qualitySourceArmHead } from "./quality-source-arm-head";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  inventory_quality_inspection_attachments: qualitySourceArmHead("\"inspection_id\",\"attachment_id\",\"ordinal\"","inventory_quality_inspection_attachments"),
  inventory_quality_inspections: qualitySourceArmHead("\"id\",\"hold_id\",\"reviewed_version\",\"result\",\"evidence\",\"inspected_by\",\"created_at\",\"spec_id\",\"spec_version\"","inventory_quality_inspections"),
} as const;
