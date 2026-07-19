export type NotificationCategory = 'action' | 'update';

export type NotificationMessageKey =
  | 'expense.submitted'
  | 'expense.departmentApproval'
  | 'expense.departmentApproved'
  | 'expense.accountingReview'
  | 'expense.accountingReviewed'
  | 'expense.accountingApproval'
  | 'expense.accountingApprovedPayment'
  | 'expense.accountingApprovedExecutive'
  | 'expense.executiveApproval'
  | 'expense.executiveApproved'
  | 'expense.payment'
  | 'expense.paymentConfirmed'
  | 'expense.settlement'
  | 'expense.completed'
  | 'expense.rejected'
  | 'expense.resubmitted'
  | 'expense.assigned'
  | 'expense.reassigned'
  | 'expense.released'
  | 'expense.reminder'
  | 'sales.submitted'
  | 'sales.salesReview'
  | 'sales.reviewed'
  | 'sales.departmentApproval'
  | 'sales.departmentApproved'
  | 'sales.autoApproved'
  | 'sales.creditCheck'
  | 'sales.creditChecked'
  | 'sales.invoice'
  | 'sales.invoiced'
  | 'sales.payment'
  | 'sales.paid'
  | 'sales.rejected'
  | 'sales.reminder';

export type NotificationArgs = Record<string, unknown>;

export interface NotificationDraft {
  key: NotificationMessageKey;
  category: NotificationCategory;
  audience: 'owner' | 'approver' | 'watcher';
  stageKey?: string | null;
  severity?: 'info' | 'success' | 'warning' | 'error';
  args: NotificationArgs;
}

function text(args: NotificationArgs, key: string, fallback = '—'): string {
  const value = args[key];
  return value == null || value === '' ? fallback : String(value);
}

export function renderNotificationMessage(key: string, args: NotificationArgs): string {
  const waybill = text(args, 'waybillId');
  const so = text(args, 'soNumber', waybill);
  const amount = text(args, 'amount');
  const counterparty = text(args, 'counterparty');
  const customer = text(args, 'customer');
  const actor = text(args, 'actor');
  const submitter = text(args, 'submitter');
  const stage = text(args, 'stage');
  const reason = text(args, 'reason');

  switch (key as NotificationMessageKey) {
    case 'expense.submitted':
      return `Expense ${waybill} for ${amount} at ${counterparty} was submitted and is waiting for Department approval.`;
    case 'expense.departmentApproval':
      return `Approval required: ${submitter} submitted expense ${waybill} for ${amount} at ${counterparty}.`;
    case 'expense.departmentApproved':
      return `Expense ${waybill} was department-approved by ${actor} and sent to Accounting review.`;
    case 'expense.accountingReview':
      return `Accounting review required: Check expense ${waybill} for ${amount} at ${counterparty} and prepare the accrual.`;
    case 'expense.accountingReviewed':
      return `Expense ${waybill} passed Accounting review by ${actor} and is waiting for Accounting approval.`;
    case 'expense.accountingApproval':
      return `Accounting approval required: Review the accrual for expense ${waybill} (${amount}).`;
    case 'expense.accountingApprovedPayment':
      return `Expense ${waybill} was approved by Accounting and sent for payment.`;
    case 'expense.accountingApprovedExecutive':
      return `Expense ${waybill} was approved by Accounting and sent for executive approval because it exceeds THB 200,000.`;
    case 'expense.executiveApproval':
      return `Executive approval required: Expense ${waybill} for ${amount} exceeds THB 200,000.`;
    case 'expense.executiveApproved':
      return `Expense ${waybill} was executive-approved by ${actor} and sent for payment.`;
    case 'expense.payment':
      return `Payment required: Pay expense ${waybill} for ${amount} to ${counterparty}.`;
    case 'expense.paymentConfirmed':
      return `Payment for expense ${waybill} was confirmed by ${actor}; Accounting settlement is pending.`;
    case 'expense.settlement':
      return `Settlement required: Post and confirm the settlement GL for expense ${waybill}.`;
    case 'expense.completed':
      return `Expense ${waybill} is complete; payment and settlement were recorded.`;
    case 'expense.rejected':
      return `Expense ${waybill} was rejected by ${actor} at ${stage}: ${reason}.`;
    case 'expense.resubmitted':
      return `Approval required: ${submitter} resubmitted corrected expense ${waybill} for ${amount}.`;
    case 'expense.assigned':
      return `Expense ${waybill} at ${stage} was assigned to you by ${actor}.`;
    case 'expense.reassigned':
      return `Expense ${waybill} at ${stage} was reassigned from you to ${text(args, 'assignee')} by ${actor}.`;
    case 'expense.released':
      return `Expense ${waybill} is available again for ${stage}.`;
    case 'expense.reminder':
      return `Reminder: Expense ${waybill} has been waiting ${text(args, 'age')} at ${stage}.`;
    case 'sales.submitted':
      return `Sales order ${so} (${waybill}) for ${customer}, ${amount}, was submitted for Sales review.`;
    case 'sales.salesReview':
      return `Sales review required: ${submitter} submitted ${so} for ${customer}, ${amount}.`;
    case 'sales.reviewed':
      return `Sales order ${so} passed Sales review by ${actor} and is waiting for Department approval.`;
    case 'sales.departmentApproval':
      return `Department approval required: Review sales order ${so} for ${customer}, ${amount}.`;
    case 'sales.departmentApproved':
      return `Sales order ${so} was department-approved by ${actor} and sent to Credit check.`;
    case 'sales.autoApproved':
      return `Sales order ${so} was auto-approved because ${amount} is below THB 5,000 and was sent to Credit check.`;
    case 'sales.creditCheck':
      return `Credit check required: Review ${customer} for sales order ${so}, ${amount}, due ${text(args, 'dueDate')}.`;
    case 'sales.creditChecked':
      return `Sales order ${so} passed Credit check by ${actor} and is ready for invoicing.`;
    case 'sales.invoice':
      return `Invoice required: Issue an invoice for sales order ${so}, ${customer}, ${amount}.`;
    case 'sales.invoiced':
      return `Invoice ${text(args, 'invoiceNumber')} was issued for sales order ${so}; customer payment is pending.`;
    case 'sales.payment':
      return `Payment follow-up: Record customer payment for invoice ${text(args, 'invoiceNumber')} on sales order ${so}.`;
    case 'sales.paid':
      return `Payment was received for sales order ${so}; ${waybill} is complete.`;
    case 'sales.rejected':
      return `Sales order ${so} was rejected by ${actor} at ${stage}: ${reason}.`;
    case 'sales.reminder':
      return `Reminder: Sales order ${so} has been waiting ${text(args, 'age')} at ${stage}.`;
    default:
      return text(args, 'message', key);
  }
}
