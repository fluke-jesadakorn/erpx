import {expect,it} from 'vitest';
import {databasePoolAcquireTimeout,databasePoolLimits,databasePoolSize} from "@erpx/feature-runtime/lib/database-pool-policy";
it('allows cold web development acquisition without changing compiled, test or jobs deadlines',()=>{
 expect(databasePoolAcquireTimeout('app','development')).toBe(20_000);
 for(const environment of ['production','test',undefined])expect(databasePoolAcquireTimeout('app',environment)).toBe(5_000);
 for(const environment of ['development','production','test',undefined])expect(databasePoolAcquireTimeout('jobs',environment)).toBe(5_000);
});
it('retains conservative compiled defaults and independent Control/jobs budgets',()=>{
 const limits=databasePoolLimits({});
 expect(databasePoolSize('H001_DB','app','production',limits)).toBe(1);
 expect(databasePoolSize('H001_DB','app','development',limits)).toBe(3);
 expect(databasePoolSize('H001_READ_DB','app','development',limits)).toBe(3);
 expect(databasePoolSize('CONTROL_READ_DB','app','development',limits)).toBe(3);
 expect(databasePoolSize('H001_DB','jobs','development',limits)).toBe(1);
 expect(databasePoolSize('CONTROL_READ_DB','app','production',limits)).toBe(1);
 expect(databasePoolSize('CONTROL_DB','app','production',limits)).toBe(5);
 expect(databasePoolSize('CONTROL_DB','jobs','production',limits)).toBe(2);
});
it('allows measured web concurrency without increasing jobs or development limits',()=>{
 const limits=databasePoolLimits({application:'4',development:'2'});
 expect(databasePoolSize('H001_DB','app','production',limits)).toBe(4);
 expect(databasePoolSize('H001_READ_DB','app','production',limits)).toBe(4);
 expect(databasePoolSize('H001_DB','jobs','production',limits)).toBe(1);
 expect(databasePoolSize('H001_DB','app','development',limits)).toBe(2);
});
for(const value of ['0','17','1.5','NaN','Infinity'])it(`rejects invalid compiled pool size ${value}`,()=>{
 expect(()=>databasePoolLimits({application:value})).toThrow('INVALID_APP_PG_POOL_MAX');
});
it('also bounds development independently',()=>{
 expect(databasePoolLimits({application:'16',development:'16'})).toEqual({application:16,development:16,control:5});
 expect(()=>databasePoolLimits({application:'4',development:'17'})).toThrow('INVALID_DEV_PG_POOL_MAX');
});
it('keeps Control concurrency independent of Holding, read bindings and workers',()=>{
 const limits=databasePoolLimits({control:'8'});
 expect(databasePoolSize('CONTROL_DB','app','production',limits)).toBe(8);
 expect(databasePoolSize('CONTROL_DB','app','development',limits)).toBe(8);
 expect(databasePoolSize('CONTROL_DB','jobs','production',limits)).toBe(2);
 expect(databasePoolSize('CONTROL_READ_DB','app','production',limits)).toBe(1);
 expect(databasePoolSize('H001_DB','app','production',limits)).toBe(1);
 for(const control of ['0','17','1.5','NaN','Infinity'])expect(()=>databasePoolLimits({control})).toThrow('INVALID_CONTROL_PG_POOL_MAX');
});
