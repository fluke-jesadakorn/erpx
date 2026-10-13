/** Fictional development identities; never import these into a production tenant. */
const rosters: Record<string,string[]> = {
  "acme-co": ["Mali Srisai","Nisa Chen","Ava Williams","Noah Patel","Henry Lawson","Leila Haddad","Owen Brooks","Sofia Ramirez","Daniel Kim","Priya Shah","James Wilson","Linh Nguyen","Amara Mensah","Ethan Clarke","Mei Tan","Isabel Santos","Arun Rao","Grace Miller","Yusuf Khan","Chloe Martin"],
  "beacon-co": ["Eli Morgan","Zoe Laurent","Oliver Hughes","Freya Andersen","Gabriel Costa","Aisha Rahman","Lucas Fischer","Hannah Lee","Mateo Silva","Emma Wright","Kenji Sato","Fatima Ali","Oscar Berg","Camila Torres","Nathan Scott","Anika Das","Louis Bernard","Sarah Cohen","Diego Flores","Eva Novak"],
  "northstar-co": ["Sam Rivera","Iris Johnson","Luca Rossi","Mia Okafor","Jun Park","Olivia Grant","Felix Weber","Layla Hassan","Hugo de Vries","Maya Thompson","Leo Dubois","Nora Jensen","Rafael Oliveira","Zara Ahmed","Adam Kowalski","Elena Popov","Isaac Brown","Sanne Bakker","Thomas Reed","Ines Moreau"],
};
const logins:Record<string,[string,string]> = {
  "Henry Lawson":["usr_henry","Sales Representative"],"Sofia Ramirez":["usr_sofia","Sales Manager"],"Leila Haddad":["usr_leila","Purchasing Specialist"],"Priya Shah":["usr_priya","Purchasing Manager"],"Owen Brooks":["usr_owen","Warehouse Manager"],"Daniel Kim":["usr_daniel","Financial Accountant"],
  "Mali Srisai":["usr_mali","People Operations Manager"],"Nisa Chen":["usr_nisa","Financial Controller"],"Ava Williams":["usr_ava","HR Administrator"],"Noah Patel":["usr_noah","Account Executive"],
  "Eli Morgan":["usr_eli","Payroll Manager"],"Zoe Laurent":["usr_zoe","Treasury Analyst"],"Sam Rivera":["usr_sam","Managing Director"],"Iris Johnson":["usr_iris","Finance Accountant"],"Luca Rossi":["usr_luca","Payroll Specialist"],"Mia Okafor":["usr_mia","People Operations Manager"],"Jun Park":["usr_jun","Internal Auditor"],"Olivia Grant":["usr_olivia","Logistics Coordinator"],
};
export function seedWorker(company:string,index:number) {
  const name=rosters[company][index]; const login=logins[name];
  return {name,userId:login?.[0]??null,email:login?`${login[0].slice(4)}@erpx.dev`:`${name.toLowerCase().replaceAll(" ",".")}@example.test`,jobTitle:login?.[1]??["Sales Coordinator","Purchasing Specialist","Warehouse Supervisor","Service Engineer","Customer Success Specialist","Implementation Consultant"][index%6]};
}
