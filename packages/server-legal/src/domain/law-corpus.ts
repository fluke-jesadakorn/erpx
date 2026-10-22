import {z} from "zod";
export const legalChunkSchema = z.object({
  page: z.number().int().min(1).max(10000),
  pageMapping: z.boolean().default(true),
  section: z.string().trim().max(200).default(""),
  text: z.string().trim().min(1).max(12000),
});
export const chunksSchema = z.array(legalChunkSchema).min(1).max(300);
export type Chunk = z.infer<typeof legalChunkSchema>;
export type ReviewRow = { id: string; version: number; state: string; created_by: string; document_id: string; document_checksum: string };

export function officialLawUrl(value: string,error:(status:number,code:string,message:string)=>Error) {
  let url: URL;
  try { url = new URL(value); } catch { throw error(422,"LEGAL_SOURCE_URL","Use the official source's HTTPS URL."); }
  if (url.protocol !== "https:" || url.username || url.password || url.port || url.hash || !url.hostname.endsWith(".go.th") || url.hostname.endsWith("."))
    throw error(422,"LEGAL_SOURCE_URL","Use an official Thai government HTTPS source without credentials, a port or a fragment.");
  return url.href;
}
export function bangkokSourceCheckDate(now:number) {
  const local = new Date(now + 7 * 3600000);
  return local.getUTCHours() < 6 ? null : local.toISOString().slice(0, 10);
}
