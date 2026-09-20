#!/usr/bin/env node
// Run an existing app encoder with this checkout's WASM and export its native model.
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL, fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
const [appsRoot, corpus, id] = process.argv.slice(2);
if (!id) throw new Error('usage: export.mjs APPS_REPO CORPUS_DIR INSTANCE_ID');
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const root = path.resolve(corpus), apps = path.resolve(appsRoot);
const item = JSON.parse(fs.readFileSync(path.join(root,'manifest.json'),'utf8')).instances.find(i=>i.id===id);
if (!item) throw new Error(`Unknown instance ${id}`);
const out = path.join(root,'parsed',id+'.json');
if (fs.existsSync(out)) throw new Error(`Refusing to overwrite ${out}`);
const esbuildPath = fs.readdirSync(path.join(apps,'node_modules/.pnpm')).filter(n=>n.startsWith('esbuild@')).sort().at(-1);
const require = createRequire(import.meta.url);
const esbuild = require(path.join(apps,'node_modules/.pnpm',esbuildPath,'node_modules/esbuild'));
const wasmDir = process.env.DEMYSTIFY_CORPUS_WASM_DIR || path.join(repo,'demystify-wasm/pkg');
const wasmPath = path.join(wasmDir,'demystify_wasm.js');
const bundles = path.join(root,'sources','bundles');fs.mkdirSync(bundles,{recursive:true});
const bundle = path.join(bundles,item.game+'.mjs');
const q = JSON.stringify;
let encoder = path.join(apps,'apps',item.game,'src/game/demystify-encode.ts');
let source;
if (item.game==='bloomsweeper') encoder=path.join(apps,'apps/bloomsweeper/src/game/solver.ts');
if (item.game==='combination') {
  source=`import {parsePieces, createBoard} from ${q(path.join(apps,'apps/combination/src/game/levelLoader.ts'))};
import {DIR_KEYS, DIR_VECTORS} from ${q(path.join(apps,'apps/combination/src/game/piece.ts'))};
import {Status} from ${q(path.join(apps,'apps/combination/src/game/types.ts'))};
import {CellStatus,buildCombinationPuzzle} from ${q(path.join(apps,'packages/combination-core/src/index.ts'))};
export function buildPuzzle(d){const board=createBoard(d),groups=new Map();
for(const p of parsePieces(d.pieces)){const shoots=DIR_KEYS.map(k=>p.direction[k]);const key=JSON.stringify([p.colour,shoots]);
if(!groups.has(key))groups.set(key,{colour:p.colour,shoots,count:0});groups.get(key).count++;}
return buildCombinationPuzzle({L:board.width,M:board.height,dirs:DIR_KEYS.map(k=>DIR_VECTORS[k]),status:(x,y)=>{
const s=board.getCell(x,y).status;return s===Status.Clear?CellStatus.Clear:s===Status.Wall?CellStatus.Wall:CellStatus.Hole;}},[...groups.values()]);}`;
} else if(item.game==='celestial') {
  source=`import {WasmBuilder} from ${q(wasmPath)};
import {validateLevel} from ${q(path.join(apps,'apps/celestial/src/game/board.ts'))};
export function buildPuzzle(d){validateLevel(d);const b=new WasmBuilder();b.kind('celestial');const N=d.size;
const cell=b.varIntMatrix('cell',[[0,N*N-1]],BigInt64Array.from([0n,1n,2n]));b.show('cell','main');
const eq=(i,v)=>cell.cell(BigInt64Array.of(BigInt(i))).eq(BigInt(v));
const guard=(id,text)=>b.guard(b.conBool(id),id,text).withMetadata({id});
for(const [v,name,count] of [[1,'sun',d.suns],[2,'moon',d.moons]]) for(const kind of ['row','column','region']) for(let j=0;j<N;j++){
const is=Array.from({length:N*N},(_,i)=>i).filter(i=>kind==='row'?Math.floor(i/N)===j:kind==='column'?i%N===j:d.regions[i]===j);
b.sumEq(guard(kind+'-'+name+'-'+j,kind+' '+j+' contains '+count+' '+name+' stars'),is.map(i=>eq(i,v)),BigInt(count));}
for(let i=0;i<N*N;i++)for(let j=i+1;j<N*N;j++)if(Math.abs(i%N-j%N)<=1&&Math.abs(Math.floor(i/N)-Math.floor(j/N))<=1){
b.table(guard('adjacent-'+i+'-'+j,'Cells '+i+' and '+j+': moons cannot touch another star'),[cell.cell(BigInt64Array.of(BigInt(i))),cell.cell(BigInt64Array.of(BigInt(j)))],[[1,2],[2,1],[2,2]]);}
return b.build();}`;
} else source=`export {buildPuzzle} from ${q(encoder)};`;
const built = await esbuild.build({stdin:{contents:source,resolveDir:apps,sourcefile:item.game+'-corpus-entry.ts',loader:'ts'},bundle:true,format:'esm',platform:'node',target:'node20',write:false,metafile:true,logLevel:'silent',plugins:[{name:'local-wasm',setup(b){
 // Newly added apps may not yet have workspace node_modules links. Resolve
 // the declared shared entry points and reuse the installed shared Zod without
 // modifying the app checkout or installing a second dependency tree.
 b.onResolve({filter:/^@bloomsweep\/shared(?:\/.*)?$/},a=>{
  const dir=path.join(apps,'packages/shared');
  const pkg=JSON.parse(fs.readFileSync(path.join(dir,'package.json'),'utf8'));
  const suffix=a.path.slice('@bloomsweep/shared'.length);
  const entry=pkg.exports[suffix?'.'+suffix:'.'];
  return typeof entry==='string'?{path:path.resolve(dir,entry)}:undefined;
 });
 b.onResolve({filter:/^zod(?:\/.*)?$/},a=>{
  for(const dir of [a.resolveDir,path.join(apps,'packages/shared')]){
   try{return {path:createRequire(path.join(dir,'__corpus-resolve.cjs')).resolve(a.path)};}catch{}
  }
  return undefined;
 });
 b.onResolve({filter:/^@bloomsweep\/demystify$/},()=>({path:'demystify-shim',namespace:'corpus'}));
 b.onLoad({filter:/.*/,namespace:'corpus'},()=>({contents:`export * from ${q(wasmPath)}; export {HintEngine} from ${q(path.join(apps,'packages/demystify/src/hint.ts'))}; export const idx=(...is)=>BigInt64Array.from(is.map(BigInt));`,resolveDir:apps}));
 b.onResolve({filter:/^@bloomsweep\/demystify\/browser$/},()=>({path:'browser',namespace:'empty-browser'}));
 b.onLoad({filter:/.*/,namespace:'empty-browser'},()=>({contents:'export async function loadDemystify(){throw new Error("Browser loader must not run in corpus exporter");}'}));
 b.onResolve({filter:/demystify_wasm\.js$/},a=>({path:pathToFileURL(wasmPath).href,external:true}));
}}]});
fs.writeFileSync(bundle,built.outputFiles[0].text);
fs.writeFileSync(path.join(bundles,item.game+'.entry.ts'),source);
fs.writeFileSync(path.join(bundles,item.game+'.inputs.json'),JSON.stringify(Object.keys(built.metafile.inputs),null,2));
const wasm = await import(pathToFileURL(wasmPath));
await wasm.default({module_or_path:fs.readFileSync(path.join(wasmDir,'demystify_wasm_bg.wasm'))});
const {buildPuzzle} = await import(pathToFileURL(bundle));
const level=JSON.parse(fs.readFileSync(path.join(root,item.input),'utf8'));
const puzzle=buildPuzzle(level);
const json=puzzle.toJson();
const planner=new wasm.WasmPlanner(puzzle,{onlyAssignments:true});
const assumptions=[];
if(item.game==='bloomsweeper')for(let r=0;r<level.height;r++)for(let c=0;c<level.width;c++)if(level.known[r][c])assumptions.push(`cell[${r},${c}]=${level.herbs[r][c]}`);
const check=planner.checkSolvability(assumptions);
const validation={...check,kind:puzzle.kind(),variables:puzzle.variables(),clauses:puzzle.numClauses(),constraints:puzzle.numConLits(),assumptions};
function save(destination, contents) {
  const temporary = destination + '.tmp';
  fs.writeFileSync(temporary, contents);
  fs.renameSync(temporary, destination);
}
save(path.join(root,'validation',id+'.json'),JSON.stringify(validation,null,2)+'\n');
if(item.game==='bloomsweeper'){
 const pinned={cell:{}};for(let r=0;r<level.height;r++)for(let c=0;c<level.width;c++)if(level.known[r][c]){pinned.cell[r]??={};pinned.cell[r][c]=level.herbs[r][c];}
 save(path.join(root,'inputs',id+'.pins.json'),JSON.stringify(pinned,null,2)+'\n');
}
// Publish the model last, so interrupted exports can be retried safely.
save(out,json);
planner.free();puzzle.free();
console.log(JSON.stringify({id,status:check.status,clauses:validation.clauses,constraints:validation.constraints}));
