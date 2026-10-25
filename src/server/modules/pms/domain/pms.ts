import { z } from "zod";
// Keep command validation out of the read-only browser model.
export { pmsSections, visiblePmsSections, pmsAccountKeys, accountLabels, records, record, plusDays, dates, amounts, money, defaultConfig, operationPermission, proportion } from "./pms-model";
export type { AccountKey, BookingImage, CollectionPolicy, BookingSettings, BookingPolicy, Config, Property, Kind, RoomType, Room, Rate, Guest, Reservation, NightPrice, Stay, GuestAccount, Entry, Task, Block, Shift, Refund, Item, Movement, Lost, Credit, Night, DataMap, PmsRecord, Snapshot } from "./pms-model";
export const textField = z.string().trim().max(2000);
export const moneyField = z.number().int().min(0).max(99999999999);
export const idField = z.string().min(1).max(128);
export const dateField = z.string().date();
export const commandSchema = z.object({
    operation: z.string().min(1).max(80),
    propertyId: z.string().max(128).optional(),
    expectedVersion: z.number().int().positive().optional(),
    attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
    input: z.unknown(),
});
