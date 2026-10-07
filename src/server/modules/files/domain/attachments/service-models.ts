import type {AttachmentComponentKind,AttachmentLinkState,AttachmentSourceState,AttachmentSetDto} from "./types";
export type SetRow = {
  id: string; company_id: string; branch_id: string | null; department_id: string | null;
  target_type: string; target_id: string; component_kind: AttachmentComponentKind; component_ref: string | null;
  purpose_key: string; expected_target_version: number; policy_version: number; classification: AttachmentSetDto["classification"];
  audience_json: string;
  required_count: number; max_files: number; state: AttachmentSetDto["state"]; created_by: string; created_at: number; updated_at: number;
};
export type LinkRow = {
  id: string; set_id: string; attachment_id: string | null; upload_intent_id: string | null; caption: string | null;
  state: AttachmentLinkState; created_by: string; created_at: number; expires_at: number; bound_at: number | null;
  original_name: string | null; media_type: string | null; declared_bytes: number | null; bytes: number | null;
  source_state: AttachmentSourceState | null; preview_object_key: string | null;
};

