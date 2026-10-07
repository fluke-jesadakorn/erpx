import {instrumentSchema,instrumentSchedule} from './finance-instrument-policy';
import {z} from 'zod';
import type {JournalLineInput} from './finance';
export const instrumentEditionSchema = instrumentSchema.safeExtend({
  periodId: z.string().min(1),
  parentId: z.string().min(1).optional(),
  parentVersion: z.number().int().positive().optional(),
  bankTransactionId: z.string().min(1).optional(),
  bankVersion: z.number().int().positive().optional(),
});




export type InstrumentRow=Record<string,string|number|null>;
export type InstrumentSchedule=ReturnType<typeof instrumentSchedule>&{rightOfUse?:string};
export type InstrumentEditionInput=z.infer<typeof instrumentEditionSchema>;
export type InstrumentEditionEvidence={calculation:InstrumentSchedule;parent:InstrumentRow|null;adjustment:bigint;bank:InstrumentRow|null;hash:string};
export type InstrumentRunEvidence={content:InstrumentEditionInput;result:InstrumentSchedule['rows'][number];ordinal:number;hash:string;lines:JournalLineInput[]};
export type InstrumentWorkspace={editions:InstrumentRow[];runs:InstrumentRow[];accounts:InstrumentRow[];periods:InstrumentRow[];bank:InstrumentRow[]};
