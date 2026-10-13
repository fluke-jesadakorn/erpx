import { z } from "zod";
import { dateField, dates, plusDays } from "./pms";
export const pmsReportRange = z.object({ propertyId: z.string().min(1), from: dateField, to: dateField, asOf:z.iso.datetime({offset:true}).optional(),compareAsOf:z.iso.datetime({offset:true}).optional() }).refine(v => v.from <= v.to && Date.parse(v.to) - Date.parse(v.from) < 93 * 86400000, { message: 'Choose up to 93 days' });
export const reportDates = (from: string, to: string) => dates(from, plusDays(to, 1));
