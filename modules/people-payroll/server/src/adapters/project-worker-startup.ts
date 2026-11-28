import {createWorkerStartup} from '@erpx/people-payroll-server/adapters/worker-startup';
const owner=createWorkerStartup('projects');
export const projectWorkerStartupWorkspace=owner.workspace,projectWorkerStartupAccounts=owner.accounts,projectWorkerStartupPeople=owner.people,prepareProjectWorkerStartup=owner.prepare,previewProjectWorkerPlacement=owner.previewPlacement,submitProjectWorkerPlacement=owner.submitPlacement,cancelProjectWorkerPlacement=owner.cancelPlacement;
