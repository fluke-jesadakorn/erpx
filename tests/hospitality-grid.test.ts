import {expect,it} from 'vitest';
import {expandRoomNumbers,roomSaveKey,mergeSetupSave,type SetupSave} from '../src/lib/hospitality-grid';
import {assertHospitalityDevelopmentTarget} from '../src/lib/hospitality-seed-target';
it('expands padded room ranges and rejects overlapping or unbounded grids',()=>{
 expect(expandRoomNumbers('001-003, 201')).toEqual(['001','002','003','201']);
 expect(()=>expandRoomNumbers('101-103, 102')).toThrow('Duplicate');
 expect(()=>expandRoomNumbers('10-1')).toThrow('ascending');
 expect(()=>expandRoomNumbers('1-1000')).toThrow('200');
});
it('preserves successful and interrupted requests while allowing rejected rows to be corrected',()=>{
 const row=(key:string,name:string):SetupSave=>({key,url:'/table/create',body:{name},label:name,state:'pending'});
 const saved={...row('saved','Original'),state:'saved' as const,resultId:'table-1'};
 const interrupted={...row('interrupted','Original'),state:'failed' as const};
 const rejected:SetupSave={...row('rejected','Invalid'),state:'failed',rejected:true};
 const queue=[saved,interrupted,rejected];
 mergeSetupSave(queue,row('saved','Edited'));
 mergeSetupSave(queue,row('interrupted','Edited'));
 mergeSetupSave(queue,row('rejected','Corrected'));
 expect(saved.body).toEqual({name:'Original'});expect(saved.resultId).toBe('table-1');
 expect(interrupted.body).toEqual({name:'Original'});
 expect(rejected.body).toEqual({name:'Corrected'});expect(rejected.requestKey).toBeTruthy();
 expect(rejected.key).toBe('rejected');expect(rejected.state).toBe('pending');
 const dependent={...row('menu','Original'),dependsOn:'product',state:'failed' as const};
 mergeSetupSave([dependent],{...row('menu','Corrected'),dependsOn:'product'});
 expect(dependent.body).toEqual({name:'Corrected'});
});
it('uses stable command keys for room labels with spaces or non-Latin characters',async()=>{
 const key=await roomSaveKey('room-row','Annex / ห้อง 1');
 expect(key).toMatch(/^[a-zA-Z0-9:_.-]{8,180}$/);
 expect(await roomSaveKey('room-row','Annex / ห้อง 1')).toBe(key);
 expect(await roomSaveKey('room-row','Annex / ห้อง 2')).not.toBe(key);
});
it('rejects every non-development database before connections',()=>{
 const manifest=(url:string)=>({databases:{H001_DB:url}});
 expect(()=>assertHospitalityDevelopmentTarget(manifest('postgresql://test@127.0.0.1:5432/erpx_dev_h001_write'),'local')).not.toThrow();
 for(const url of ['postgresql://test@127.0.0.1:5432/erpx_h001_write','postgresql://test@127.0.0.1:5432/erpx_stage_h001_write','postgresql://test@remote:5432/erpx_dev_h001_write','postgresql://test@127.0.0.1:5433/erpx_dev_h001_write'])expect(()=>assertHospitalityDevelopmentTarget(manifest(url),'local')).toThrow('HOSPITALITY_DEVELOPMENT_ONLY');
 expect(()=>assertHospitalityDevelopmentTarget(manifest('postgresql://test@127.0.0.1:5432/erpx_dev_h001_write'),'staging')).toThrow();
});
