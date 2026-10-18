'use client';
import {createContext,useContext,type ReactNode} from 'react';
import {InlineNotice} from '@/components/ui/foundation';
import type {ExpensePurchaseControls} from '@/lib/people_payroll/expense-purchases';
const Purchase=createContext<ExpensePurchaseControls|undefined>(undefined);
export function ExpensePurchaseProvider({controls,children}:{controls?:ExpensePurchaseControls;children:ReactNode}){return <Purchase.Provider value={controls}>{children}</Purchase.Provider>;}
export function useExpensePurchase(){return useContext(Purchase);}
export function ExpenseNewWork({children}:{children:ReactNode}){const purchase=useExpensePurchase();return purchase&&!purchase.newWork?<InlineNotice tone="info">New Expense work is unavailable. Retained claims remain available for their permitted settlement and history.</InlineNotice>:children;}
