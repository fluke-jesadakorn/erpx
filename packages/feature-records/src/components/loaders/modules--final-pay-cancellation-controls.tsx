'use client';
import dynamic from 'next/dynamic';
export const FinalPayCancellationControls=dynamic(()=>import('@erpx/feature-people_payroll/components/final-pay-cancellation-controls').then(module=>module.FinalPayCancellationControls));
