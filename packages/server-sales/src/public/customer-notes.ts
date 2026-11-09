export {
  nativeCustomerNotes,
  nativeCustomerNoteCommand,
  nativeCustomerNotesAvailable,
} from '@erpx/server-customer_360/public/customer-notes';
export { customerNoteSchema, customerNoteChangeSchema } from '@erpx/server-sales/domain/notes';
export type { CustomerNotesView } from '@erpx/server-sales/domain/notes';
export { customerNotesForOrder, customerNotesForCustomer, createCustomerNote, changeCustomerNote } from '@erpx/server-sales/adapters/notes';
