import {workTaskSocialQueuedSourceScope as sourceScope} from '@erpx/server-collaboration/adapters/work-task-queued-read-sources';
export function workTaskSocialQueuedSourceScope(input:Parameters<typeof sourceScope>[0]){return sourceScope(input);}
