import {z} from "zod";
export const handbookSchema = z.object({
  title: z.string().trim().max(200).default(""),
  category: z
    .enum(["policy", "procedure", "faq", "service_guide"])
    .default("procedure"),
  body: z.string().max(40000).default(""),
  expectedVersion: z.number().int().positive().default(1),
});
export const handbookActionSchema = z.object({
  action: z.enum(["publish", "archive", "restore"]),
  expectedVersion: z.number().int().positive(),
});
export const handbookEditorSchema = z.object({
  userId: z.string().min(1).max(160),
  active: z.boolean(),
  expectedVersion: z.number().int().min(0),
});
export type Article = {
  id: string;
  company_id: string;
  version: number;
  draft_revision: number;
  published_revision: number | null;
  archived: number;
  created_by: string;
  updated_at: number;
};
export type HandbookRevision = {
  revision: number;
  title: string;
  category: string;
  body: string;
  created_by: string;
  created_at: number;
  published_by: string | null;
  published_at: number | null;
};
export type HandbookListRow={id:string;page_key:string;version:number;archived:number;revision:number;title:string;category:string;excerpt:string;published_at:number|null};
