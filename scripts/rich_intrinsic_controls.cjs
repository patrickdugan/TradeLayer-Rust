const fs=require('fs'),path=require('path'),vm=require('vm'),assert=require('assert/strict');
const names=['Object','Array','Function','Error','TypeError','RangeError','SyntaxError','ReferenceError','URIError','EvalError','AggregateError','Boolean','Number','String','BigInt','Symbol'],records=[];
for(const realm of ['HostIO','SourceVM']){const b=realm==='HostIO'?globalThis:vm.runInNewContext('({'+names.join(',')+'})');
 for(const name of names)for(const suffix of ['', '.prototype']){const value=suffix?b[name].prototype:b[name];
  for(const operation of ['implicit','brand','name','length','functionText']){let returned,thrown;try{returned=operation==='implicit'?`${value}`:operation==='brand'?b.Object.prototype.toString.call(value):operation==='functionText'?b.Function.prototype.toString.call(value):value[operation];}catch(error){assert.ok(Object.getPrototypeOf(error)===b.TypeError.prototype);thrown={constructor:'TypeError',realm,message:Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i))};}
   const result=thrown?{thrown}:{returned:returned===undefined?{tag:'Undefined'}:typeof returned==='string'?{tag:'UTF16',units:Array.from({length:returned.length},(_,i)=>returned.charCodeAt(i))}:typeof returned==='number'?{tag:'Binary64',bits:(()=>{let b=Buffer.alloc(8);b.writeDoubleBE(returned);return b.toString('hex')})()}:(()=>{throw Error('unsupported native result')})()};
   records.push({realm,intrinsic:name+suffix,operation,...result});
  }
 }
}
const out=JSON.stringify(records,null,2)+'\n',f=path.join(__dirname,'../fixtures/rich_intrinsic_controls.json');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(f,'utf8'),out);else fs.writeFileSync(f,out);console.log(JSON.stringify({nativeControls:records.length,node:process.version}));
