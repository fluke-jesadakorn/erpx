'use client';
import dynamic from 'next/dynamic';
export const PayrollOtherRemittancesPanel=dynamic(()=>import('@erpx/people-payroll-ui/components/payroll-other-remittances-panel').then(module=>module.PayrollOtherRemittancesPanel));
