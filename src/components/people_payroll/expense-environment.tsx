import type {ReactNode} from 'react';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {expensePurchaseControls} from '@/server/modules/people_payroll/public/expense-purchases';
import {ExpensePurchaseProvider} from './expense-purchase-context';
export async function ExpenseEnvironment({actor,children}:{actor:WorkContext;children:ReactNode}){return <ExpensePurchaseProvider controls={await expensePurchaseControls(actor)}>{children}</ExpensePurchaseProvider>;}
