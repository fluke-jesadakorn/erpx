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
  onAcknowledged?: () => void,
  onEditableRejected?: () => void,
) => Promise<boolean>;
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
