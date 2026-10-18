'use client';
import dynamic from 'next/dynamic';
export const ExpensePostingSetup=dynamic(()=>import('@/components/finance/expense-posting-setup').then(module=>module.ExpensePostingSetup));
