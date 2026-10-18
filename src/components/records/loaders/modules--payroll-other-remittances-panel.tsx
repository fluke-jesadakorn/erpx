'use client';
import dynamic from 'next/dynamic';
export const PayrollOtherRemittancesPanel=dynamic(()=>import('@/components/people_payroll/payroll-other-remittances-panel').then(module=>module.PayrollOtherRemittancesPanel));
