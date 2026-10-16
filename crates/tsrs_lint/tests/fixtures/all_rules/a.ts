export {};
declare const anyValue:any;
declare const maybeString:string|undefined;
declare const promise:Promise<number>;
enum E {A,B}
class C {
 private value=1;
 method(){return this.value;}
 get item():string {return '';}
 set item(value:number){}
 async idle(){return 1;}
}
const c=new C();
const detached=c.method;
const numbers=[1,2];
numbers.sort();
const found=numbers.filter(x=>x>0)[0];
const has=numbers.indexOf(1)!==-1;
const text=`${anyValue}${c}${numbers}`;
const fallback=(maybeString && maybeString.trim()) || 'fallback';
const sum:any=anyValue+1;
async function f(){return promise;}
const cb=promise.catch(error=>{console.log(error.message);throw error;});
const caught:Promise<void>=promise.then(()=>{},(err:any)=>{throw err;});
function acceptsVoid(callback:()=>void){callback();}
acceptsVoid(()=>1);
const truthy=anyValue ? true : false;
switch(E.A as E){case E.A:break;}
new Promise((resolve,reject)=>reject('x'));
void Promise.resolve(1);
