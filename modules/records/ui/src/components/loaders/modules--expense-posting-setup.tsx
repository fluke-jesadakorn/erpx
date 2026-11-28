'use client';
import dynamic from 'next/dynamic';
export const ExpensePostingSetup=dynamic(()=>import('@erpx/finance-ui/components/expense-posting-setup').then(module=>module.ExpensePostingSetup));
