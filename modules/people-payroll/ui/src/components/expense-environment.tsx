import type {ReactNode} from 'react';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {expensePurchaseControls} from '@erpx/people-payroll-server/public/expense-purchases';
import {ExpensePurchaseProvider} from '@erpx/people-payroll-ui/components/expense-purchase-context';
export async function ExpenseEnvironment({actor,children}:{actor:WorkContext;children:ReactNode}){return <ExpensePurchaseProvider controls={await expensePurchaseControls(actor)}>{children}</ExpensePurchaseProvider>;}
