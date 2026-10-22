import { z } from "zod";
export function drilldownPage(value?: string) { return z.coerce.number().int().min(1).max(100000).default(1).parse(value); }
