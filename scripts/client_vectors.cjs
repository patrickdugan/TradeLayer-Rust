'use strict';
// Actual pinned client.js, with RPC/library/OS boundaries captured rather than
// invoking wallets or opening any network connection.
const fs=require('fs'),path=require('path'),vm=require('vm'),crypto=require('crypto'),util=require('util');
const root=path.resolve(__dirname,'..');
const archive=path.join(root,'fixtures','legacy_client_source.txt');
const source=fs.readFileSync(archive);
const sha='dc8cf76301825e865a92d7b0e6028ccbe06c600a79e1e5cdcf59ac366cc6212e';
if(crypto.createHash('sha256').update(source).digest('hex')!==sha)throw Error('source SHA mismatch');
const sourcePath=process.argv.slice(2).find(arg=>!arg.startsWith('--'));
if(sourcePath) {
 const given=fs.readFileSync(sourcePath);
 if(!given.equals(source))throw Error('pinned source bytes differ');
}
function load(env={}) {
 const calls=[];
 function Client(config) {
  this.config=config;
  this.cmd=(method,...args)=>{
   const callback=args.pop();
   const observation={method,params:args,path:null};calls.push(observation);callback(null,observation);
  };
  this.rpc={call:(method,params,ok,fail,walletPath)=>{
   const observation={method,params,path:walletPath};calls.push(observation);ok(observation);
  }};
 }
 const context={module:{exports:{}},exports:{},process:{env:{...env}},__dirname:'/pinned/src',setTimeout,console:{log(){},warn(){},error(){}},require(name){
  if(name==='util')return util;
  if(name==='path')return path;
  if(['litecoin','bitcoin','dogecoind-rpc'].includes(name))return {Client};
  if(name==='dotenv')return {config(){}};
  throw Error('unexpected dependency '+name);
 }};
 vm.runInNewContext(source.toString('utf8'),context,{filename:'client.js',timeout:1000});
 return {Class:context.module.exports,calls,Client};
}
async function main() {
 const rows=[];
 const add=(kind,input,output)=>rows.push([kind,JSON.stringify(input),JSON.stringify(output)].join('\t'));
 const bootstrap=[
  ['LTC',{}],['LTCTEST',{}],['BTC',{}],['DOGE',{}],['BTCTEST',{}],['DOGETEST',{}],
  ['LTC',{CHAIN:'btc'}],['LTC',{CHAIN:'doge',RPC_HOST:'::1',TIMEOUT_MS:'2500'}],
  ['LTC',{CHAIN:'BTCTEST'}],['BTC',{AUTODETECT:'0'}],['LTC',{RPC_PORT:'19443'}],
  ['LTC',{RPC_PORT:'0x208d'}],['LTC',{RPC_PORT:'1e4',TIMEOUT_MS:'3e3'}],
  ['LTC',{RPC_WALLET:'first',LTC_WALLET:'second',WALLET_NAME:'third'}],
  ['LTC',{LTC_WALLET:'second',WALLET_NAME:'third'}],['LTC',{WALLET_NAME:'third'}],
 ];
 for(const [defaultChain,env] of bootstrap){
  const {Class}=load(env);const client=new Class();const boot=client.getEnvBootstrap(defaultChain);
  add('B',{defaultChain,env},{chain:boot.chain,host:boot.host,port:boot.port,timeout:boot.timeout,locked:boot.locked,wallet:client.getWalletPath()});
 }
 const methods=[
  ['getBlockchainInfo',[]],['getRawTransaction',['tx']],['getRawTransaction',['tx',false,'ignored']],
  ['getnewaddress',[]],['sendtoaddress',['destination',1.25]],['getNetworkInfo',[]],
  ['getTransaction',['tx']],['getBlock',['hash']],['getBlockHash',[123]],
  ['createRawTransaction',[[{txid:'prev',vout:0}],{dest:1}]],['listUnspent',[6,100,['address']]],
  ['decoderawtransaction',['00']],['signrawtransactionwithwallet',['00']],['dumpprivkey',['address']],
  ['sendrawtransaction',['00']],['validateAddress',['address']],['getBlockCount',[]],
  ['loadWallet',['wallet']],['listlabels',['receive']],['getaddressesbylabel',['label']],['getwalletinfo',[]],
  ['listunspent',[99]],
 ];
 for(const wallet of [null,'wallet/a b?#雪'])for(const [wrapper,args] of methods){
  const env=wallet?{RPC_WALLET:wallet}:{};const {Class,Client}=load(env);const client=new Class();client.client=new Client({});
  add('R',{wrapper,args,wallet},await client[wrapper](...args));
 }
 for(const version of ['/LitecoinCore:1/','/BitcoinCore:1/','/Dogecoin:1/','/Satoshi:1/','/Shibetoshi:1/','bitcoin+litecoin','unknown']){
  const {Class}=load();let result;
  try{result=new Class().determineChainFromSubversion(version)}catch{result=null}
  add('C',version,result);
 }
 // The real source never clears this flag, even when a locked init succeeds.
 const {Class}=load({CHAIN:'LTC'});const client=new Class();const initialized=await client.init();
 add('OBS','locked-init',{chain:initialized,isInitializing:client.isInitializing});
 const text='# pinned client.js source observations; SHA256 '+sha+'\n'+rows.join('\n')+'\n';
 const fixture=path.join(root,'fixtures','client_vectors.tsv');
 if(process.argv.includes('--write')){fs.writeFileSync(fixture,text);console.log('Wrote '+rows.length+' client observations');}
 else {if(fs.readFileSync(fixture,'utf8')!==text)throw Error('client corpus mismatch');console.log(rows.length+' client source observations passed');}
}
main().catch(error=>{console.error(error);process.exitCode=1});
