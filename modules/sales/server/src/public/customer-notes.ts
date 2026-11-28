export {
  nativeCustomerNotes,
  nativeCustomerNoteCommand,
  nativeCustomerNotesAvailable,
} from '@erpx/customer-360-server/public/customer-notes';
export { customerNoteSchema, customerNoteChangeSchema } from '@erpx/sales-server/domain/notes';
export type { CustomerNotesView } from '@erpx/sales-server/domain/notes';
export { customerNotesForOrder, customerNotesForCustomer } from '@erpx/sales-server/adapters/notes';
