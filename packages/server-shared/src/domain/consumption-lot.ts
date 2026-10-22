import { z } from "zod";
/** Allocation evidence shared by the Inventory, Maintenance and Field Service owners. */
export const consumptionLotSchema = z.object({ lotId: z.string().min(1), quantityMilli: z.number().int().positive().safe(), balanceVersion: z.number().int().positive() });
export type ConsumptionLot = z.infer<typeof consumptionLotSchema>;
