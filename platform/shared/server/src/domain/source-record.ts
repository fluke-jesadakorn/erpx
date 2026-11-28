import {z} from "zod";
export const sourceRecordKind=z.enum(["commerce_order","support_ticket","field_service_order","staff_task","pos_sale","pms_record","asset","maintenance_plan","maintenance_work"]);
export const sourceRecordInput=z.object({kind:sourceRecordKind,id:z.string().min(1).max(180),version:z.number().int().positive().optional()});
