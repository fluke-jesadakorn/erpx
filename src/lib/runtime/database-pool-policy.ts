/** Per-binding limits; jobs and Control retain their separate budgets. */
export function databasePoolLimits(values:{application?:string;development?:string;control?:string}) {
 const application=Number(values.application??1),development=Number(values.development??3),control=Number(values.control??5);
 if(!Number.isInteger(application)||application<1||application>16)throw new Error('INVALID_APP_PG_POOL_MAX');
 if(!Number.isInteger(development)||development<1||development>16)throw new Error('INVALID_DEV_PG_POOL_MAX');
 if(!Number.isInteger(control)||control<1||control>16)throw new Error('INVALID_CONTROL_PG_POOL_MAX');
 return {application,development,control};
}
export function databasePoolSize(binding:string,role:'app'|'jobs',environment:string|undefined,limits:ReturnType<typeof databasePoolLimits>) {
 if(binding==='CONTROL_DB')return role==='jobs'?2:limits.control;
 if(role==='jobs')return 1;
 return environment==='development'?limits.development:limits.application;
}
