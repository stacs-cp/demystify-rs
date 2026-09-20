#!/usr/bin/env python3
"""Select small shipped levels across source difficulty tiers, retaining provenance."""
import argparse,collections,datetime,hashlib,json,pathlib,re,shutil
p=argparse.ArgumentParser();p.add_argument('--apps-root',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);args=p.parse_args()
root=args.out.resolve();apps=args.apps_root.resolve()
root.mkdir(parents=True,exist_ok=True)
assert not (root/'manifest.json').exists(),'Manifest exists; use a new corpus directory'
for folder in ['inputs','parsed','graphs','viewers','logs','validation','sources']:(root/folder).mkdir(exist_ok=True)
def area(d):
 b=d.get('board',{});b=b if isinstance(b,dict) else {}
 return d.get('width',0)*d.get('height',0) or d.get('size',0)**2 or b.get('L',0)*b.get('M',0) or 1+3*d.get('radius',0)*(d.get('radius',0)+1)
def tier(d,pack):
 m=d.get('meta',{});t=m.get('mus',m.get('target_mus',m.get('difficulty',{}).get('mus_quality')))
 if t is None:
  match=re.search(r'mus0?(\d+)',pack);t=int(match[1]) if match else None
 return t
inventory={};selected=[]
for app in sorted((apps/'apps').iterdir()):
 if not app.is_dir():continue
 game=app.name;pool=[]
 for f in sorted(app.glob('src/game/levelpacks/*/*.json')):
  d=json.loads(f.read_text());pack=f.parent.name
  pool.append(dict(game=game,path=f,pack=pack,data=d,tier=tier(d,pack),area=area(d),name=f.stem))
 if game=='combination':
  for f in sorted(app.glob('public/assets/levels/*.json')):
   for k,d in enumerate(json.loads(f.read_text())):
    if isinstance(d['grid'],str):a=sum(len(r) for r in d['grid'].split('|'))
    else:a=sum(len(col) for col in d['grid'])
    pool.append(dict(game=game,path=f,pack=f.stem,data=d,tier=None,area=a,name=str(k+1),source_index=k))
 if not pool:continue
 inventory[game]={'available_levels':len(pool),'packs':dict(collections.Counter(x['pack'] for x in pool))}
 groups={'main':pool}
 if game=='bloomsweeper':groups={pack:[x for x in pool if x['pack']==pack] for pack in sorted({x['pack'] for x in pool})}
 if game=='gamut':groups={v:[x for x in pool if x['pack'].startswith(v)] for v in ['plain','tinted']}
 if game=='sashes':groups={v:[x for x in pool if x['pack'].startswith('hex')==(v=='hex')] for v in ['rhombus','hex']}
 if game=='fell':groups={s:[x for x in pool if x['pack']==s] for s in ['4x4-h4','4x5-h5']}
 if game=='combination':groups={s:[x for x in pool if x['pack']==s] for s in ['Simple','Tricky','SuperHard']}
 if game=='poly-pic':groups={s:[x for x in pool if x['pack']==s] for s in ['tutorial','all-ls','l-s-z']}
 for variant,candidates in groups.items():
  if not candidates:continue
  # Prefer small boards; if fewer than 3 source tiers fit, admit the smallest
  # examples of the next tier instead of inventing a difficulty label.
  smallest=min(x['area'] for x in candidates);cap=max(36,smallest)
  eligible=[x for x in candidates if x['area']<=cap]
  tiers=sorted({x['tier'] for x in eligible if x['tier'] is not None})
  if tiers:
   if len(tiers)<3 and game!='bloomsweeper':
    rest=sorted({x['tier'] for x in candidates if x['tier'] is not None}-set(tiers),key=lambda t:min(x['area'] for x in candidates if x['tier']==t))
    for t in rest[:3-len(tiers)]:
     size=min(x['area'] for x in candidates if x['tier']==t);eligible.extend(x for x in candidates if x['tier']==t and x['area']==size)
    tiers=sorted({x['tier'] for x in eligible if x['tier'] is not None})
   wanted=sorted(set([tiers[0],tiers[-1]] if game=='bloomsweeper' else [tiers[0],tiers[len(tiers)//2],tiers[-1]]))
   picks=[min((x for x in eligible if x['tier']==t),key=lambda x:(x['area'],x['pack'],x['name'])) for t in wanted]
  else:
   count=2 if game in ['celestial','sashes'] else 1 if game in ['combination','poly-pic'] else 3
   picks=sorted(eligible,key=lambda x:(x['area'],x['pack'],x['name']))[:count]
  for x in picks:
   id=re.sub(r'[^a-zA-Z0-9_.-]+','-',f"{game}-{x['pack']}-{x['name']}")
   out=root/'inputs'/f'{id}.json'
   out.write_text(json.dumps(x['data'],indent=2)+'\n')
   selected.append({'id':id,'game':game,'variant':variant,'source_pack':x['pack'],'source_tier':x['tier'],'board_area':x['area'],
                    'input':str(out.relative_to(root)),'input_sha256':hashlib.sha256(out.read_bytes()).hexdigest(),
                    'original_path':str(x['path']),'original_sha256':hashlib.sha256(x['path'].read_bytes()).hexdigest(),
                    **({'original_array_index':x['source_index']} if 'source_index' in x else {})})
manifest={'schema_version':1,'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'apps_root':str(apps),
          'selection':'Small shipped boards at low/middle/high available source tiers; two endpoints for each Bloomsweeper variant. Non-numeric packs are labelled without invented difficulty. The smallest third tier may exceed the preferred 36-cell cap.',
          'settings':{'repeats':5,'strategy':'dynamic','conflict_limit':1000,'only_assign':True,'threads':2,'checkpoint_seconds':10},
          'inventory':inventory,'instances':selected}
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(root, len(selected),'instances across',len(inventory),'games')
for game in inventory:
 print(game,[(x['source_pack'],x['source_tier'],x['board_area']) for x in selected if x['game']==game])
