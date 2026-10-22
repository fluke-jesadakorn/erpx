import {z} from 'zod';
export const expensePaymentCancellationSchema=z.object({
 expectedVersion:z.number().int().positive(),expenseVersion:z.number().int().positive(),
 reason:z.string().trim().min(3).max(2000),paymentNotExecuted:z.literal(true),
}).strict();
export type ExpensePaymentCancellationInput=z.infer<typeof expensePaymentCancellationSchema>;
export type ExpensePaymentCancellationResult={id:string;expenseId:string;status:'cancelled';version:number;expenseVersion:number;reason:string;paymentNotExecuted:true};
