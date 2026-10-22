import type {PoolClient} from 'pg';
/** Read through the IAM caller's supplied repeatable-read or command transaction. */
export async function directoryMappingPerson(client:PoolClient,companyId:string,personId:string){return (await client.query<{legal_name:string}>('SELECT legal_name FROM hr_people WHERE id=$1 AND company_id=$2 AND merged_into_id IS NULL',[personId,companyId])).rows[0]??null;}
