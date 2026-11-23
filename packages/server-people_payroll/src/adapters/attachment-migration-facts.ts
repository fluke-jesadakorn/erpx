import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativePeoplePayrollReadAll,PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS} from './native-people-payroll-read';
export function attachmentMigrationpeoplepayroll(database:SqlDatabase){return {
readReconcileAttachmentStoragePaymentExports(){return nativePeoplePayrollReadAll<{id:string;file_key:string;file_checksum:string|null}>(PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,[],database);},
readReconcileAttachmentStoragePayrollReviewExports(){return nativePeoplePayrollReadAll<{id:string;file_key:string;checksum_sha256:string}>(PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS,[],database);}};}
