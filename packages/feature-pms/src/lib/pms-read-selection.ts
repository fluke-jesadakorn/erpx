import { canonicalGuestAccount } from "@erpx/feature-identity_access/lib/guest-account-compat";
import { z } from "zod";
import { dateField, idField } from "@erpx/server-pms/public/rules/pms";

export const pmsListSections = ["reservations", "front-desk", "rooms", "housekeeping", "guest-accounts", "cashier", "inventory", "guests", "night-audit"] as const;

export const pmsReadSelectionSchema = z.discriminatedUnion("view", [
  z.object({ view: z.literal("dashboard") }),
  z.object({ view: z.literal("setup") }),
  z.object({ view: z.literal("report") }),
  z.object({view:z.literal("choices"),kind:z.enum(["guest","block"]),q:z.string().trim().max(200).default(""),selectedId:idField.optional()}),
  z.object({ view: z.literal("calendar"), from: dateField, to: dateField }).refine(value =>
    value.from < value.to && Date.parse(value.to) - Date.parse(value.from) <= 93 * 86_400_000,
    { message: "Choose a calendar range of at most 93 days." }),
  z.object({ view: z.literal("detail"), id: idField }),
  z.object({view:z.literal("list"),section:z.enum(pmsListSections),from:dateField.optional(),to:dateField.optional(),page:z.coerce.number().int().min(0).max(10000).default(0),q:z.string().trim().max(200).default(""),status:z.enum(["","arrivals","departures","reserved","in_house","cancelled","no_show","completed","checked_out","payment_pending","confirmed","open","closed","reviewed","dirty","clean","inspected","running","blocked"]).default(""),building:z.string().trim().max(120).optional(),floor:z.string().trim().max(40).optional(),typeId:z.union([z.literal(""),idField]).optional(),selectedId:idField.optional()}).refine(v=>!v.from||!v.to||v.from<=v.to&&Date.parse(v.to)-Date.parse(v.from)<=93*86400000,{message:"Choose a list date range of at most 93 days."}),
]);
export type PmsReadSelection = z.infer<typeof pmsReadSelectionSchema>;

export function pmsReadSelectionFromSearch(search: URLSearchParams) {
  if (!search.has("view")) return undefined;
  return pmsReadSelectionSchema.parse(canonicalGuestAccount({ view: search.get("view"), kind:search.get("kind"), from: search.get("from")??undefined,
    to: search.get("to")??undefined, id: search.get("id"),section:search.get("section"),page:search.get("page")??0,q:search.get("q")??"",status:search.get("status")??"",building:search.get("building")??"",floor:search.get("floor")??"",typeId:search.get("typeId")??"",selectedId:search.get("selectedId")??undefined }));
}

export function pmsListSelection(section:string,search:Record<string,string|undefined>){
 if(section==='settings')return pmsReadSelectionSchema.parse({view:'setup'});
 if(section==='reports')return pmsReadSelectionSchema.parse({view:'report'});
 if(!pmsListSections.some(value=>value===section))return undefined;
 const queue=section==='front-desk'?search.queue:undefined;
 return pmsReadSelectionSchema.parse({view:"list",section,from:search.from,to:search.to,page:search.page??0,q:search.q??"",status:search.status??queue??"",building:search.building??"",floor:search.floor??"",typeId:search.typeId??"",selectedId:search.detailKind==="pms_record"?search.detailId:search.record??search.guestAccount??search.task??search.night});
}
