import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeRuntimeStatement } from '@erpx/platform-runtime-ui/adapters/server/native-runtime-command';
/** Effects bind the caller's transaction; the native axum executor is now the only source of this table mutation. No SQL remains in TypeScript. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBinding(holding_id:string,slug:string,company_name:string,writeBinding:string,read:string,files:string){return nativeRuntimeStatement('runtime_stmt_18', { companyId: '', now: Date.now(), actor: undefined })
      .bind(
        holding_id,
        slug,
        company_name,
        writeBinding,
        read,
        files,
      );}};}
