import { getRuntime,closeRuntime } from "../../src/server/modules/runtime/adapters/runtime/env";
import { bootstrapAdmin } from './bootstrap-admin-service';
export { bootstrapAdmin } from './bootstrap-admin-service';
if(import.meta.main){try{
  const env=getRuntime();
  const email=process.argv[2]??process.env.ERPX_ADMIN_EMAIL;
  if(!email)throw new Error('ERPX_ADMIN_EMAIL_REQUIRED');
  console.log(await bootstrapAdmin(env.CONTROL_DB,email,env.H003_DB));
}finally{await closeRuntime();}}
