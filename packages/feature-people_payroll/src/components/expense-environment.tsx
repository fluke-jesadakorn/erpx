import type {ReactNode} from 'react';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {expensePurchaseControls} from '@erpx/server-people_payroll/public/expense-purchases';
import {ExpensePurchaseProvider} from '@erpx/feature-people_payroll/components/expense-purchase-context';
export async function ExpenseEnvironment({actor,children}:{actor:WorkContext;children:ReactNode}){return <ExpensePurchaseProvider controls={await expensePurchaseControls(actor)}>{children}</ExpensePurchaseProvider>;}
