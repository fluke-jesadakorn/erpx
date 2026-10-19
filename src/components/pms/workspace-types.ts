import type {ReactNode} from "react";
export type Option = { id: string; name: string };
export type Field = {
  name: string;
  label: string;
  type?:
    "text" | "date" | "time" | "number" | "money" | "textarea" | "checkbox" | "select" | "email";
  options?: Option[];
  value?: string | number | boolean;
  required?: boolean;
  min?: number;
};
export type Send = (
  op: string,
  input: unknown,
  version?: number,
  attachmentSetIds?: string[],
  onAcknowledged?: (refreshed: boolean) => void,
  onEditableRejected?: () => void,
) => Promise<boolean>;
export type PmsTaskCompletionDraftAttachment = {
  purposeKey: string;
  setId: string | null;
  readyCount: number;
  requiredCount: number;
  bindable: boolean;
  links: {
    id: string;
    filename: string;
    mediaType: string;
    bytes: number | null;
    sourceState: string | null;
    linkState: string;
    caption: string | null;
    previewAvailable: boolean;
  }[];
};
export type PmsTaskCompletionDraft = {
  key: string;
  base: string;
  attachmentBase: string;
  propertyId: string;
  taskId: string;
  taskLabel: string;
  checklist: { label: string; checked: boolean }[];
  notes: string;
  attachments: PmsTaskCompletionDraftAttachment[];
  editing: boolean;
};
export type PmsTaskCompletionDraftValues = Pick<PmsTaskCompletionDraft, "checklist" | "notes" | "attachments">;
export function changedPmsTaskCompletionAttachments(
  submitted: ReadonlyMap<string, PmsTaskCompletionDraftAttachment>,
  current: ReadonlyMap<string, PmsTaskCompletionDraftAttachment>,
) {
  return [...current.entries()]
    .filter(([purposeKey, attachment]) => JSON.stringify(attachment) !== JSON.stringify(submitted.get(purposeKey)))
    .map(([, attachment]) => attachment);
}
export type ActionAttachment = {
  base: string;
  targetType: string;
  targetId: string;
  expectedTargetVersion: number;
  purposeKey: string;
  title: string;
  description: string;
};
export type PmsForm = (title:string,op:string,fields?:Field[],hidden?:Record<string,unknown>,transform?:(value:Record<string,unknown>)=>unknown,attachments?:ActionAttachment[])=>ReactNode;
