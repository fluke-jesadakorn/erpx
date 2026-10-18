'use client';
import dynamic from 'next/dynamic';
export const PayrollPostingPrerequisites=dynamic(()=>import('@/components/people_payroll/payroll-posting-prerequisites').then(module=>module.PayrollPostingPrerequisites));
export const PayrollPostingSettingsLink=dynamic(()=>import('@/components/people_payroll/payroll-posting-prerequisites').then(module=>module.PayrollPostingSettingsLink));
