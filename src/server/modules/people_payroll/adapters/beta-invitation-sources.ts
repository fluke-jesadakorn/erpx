import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {PoolClient} from 'pg';

export function betaInvitationSources(db:SqlDatabase,c?:PoolClient){return {async existingMember(companyId:string,email:string){return !!(await c!.query(
            "SELECT 1 FROM hr_people WHERE company_id=$1 AND lower(email)=$2 AND user_id IS NOT NULL",
            [companyId, email],
          )).rowCount;}};}
