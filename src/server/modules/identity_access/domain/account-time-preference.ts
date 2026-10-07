import {z} from 'zod';
import {validTimeZone} from '@/lib/ui-format';
export type {TimePreference} from '@/lib/ui-format';
export {defaultTimePreference} from '@/lib/ui-format';

export const accountTimePreferenceInput = z.object({
  mode: z.enum(['system', 'manual']),
  timeZone: z.string().max(80).nullable(),
  hourCycle: z.enum(['h12', 'h23']),
  version: z.number().int().min(0),
}).strict().refine(value => value.mode === 'system' ? value.timeZone === null : !!value.timeZone && validTimeZone(value.timeZone), {
  message: 'Choose a valid IANA timezone for Manual mode.',
});
export type AccountTimePreferenceInput = z.infer<typeof accountTimePreferenceInput>;
