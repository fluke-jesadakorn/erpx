import {z} from "zod";
export const dateFilters = z.object({
  from: z.iso.date(),
  to: z.iso.date(),
}).strict().refine(({ from, to }) => from <= to && Date.parse(to) - Date.parse(from) <= 366 * 86_400_000, "Choose a date range of up to one year.");

export type Group = { label: string; value: string };

export type PlatformExecutiveSnapshot = {
  scope: "platform";
  referenceAt: number;
  filters: { from: string; to: string };
  sourceRevision: string;
  metrics: {
    id: "failed-provisioning" | "abnormal-platform-services";
    label: string;
    formula: string;
    dateBasis: string;
    total: string;
    rows: Group[];
    status: "ready" | "empty";
    sourceReferences: { dataset: string; revision: string; referencedAt: number }[];
  }[];
};

export const executiveSourceSchema = z.object({metric:z.enum(["failed-provisioning","abnormal-platform-services"]),label:z.string().min(1).max(250),from:z.iso.date(),to:z.iso.date(),page:z.coerce.number().int().min(1).max(100000).default(1)}).strict();
export function metricTotal(rows:Group[],error:(status:number,code:string,message:string)=>Error) {
 const exact=(value:string|number|bigint)=>{if(typeof value==='bigint')return value;if(typeof value==='number'){if(!Number.isSafeInteger(value))throw error(500,'PLATFORM_METRIC_NOT_EXACT','The control plane returned a value that cannot be represented exactly.');return BigInt(value);}if(!/^-?\d+$/.test(value))throw error(500,'PLATFORM_METRIC_INVALID','The control plane returned an invalid metric value.');return BigInt(value);};
 return String(rows.reduce((sum,row)=>sum+exact(row.value),BigInt(0)));
}
