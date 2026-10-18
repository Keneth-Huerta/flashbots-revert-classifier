# Analisis exploratorio de la muestra Dune 7843588 (32k filas). Uso: python3 -I scripts/analisis/eda_muestra_32k.py <csv>
import csv,sys,collections as C,statistics as S
rows=list(csv.DictReader(open(sys.argv[1])))
print("rows",len(rows))
bots=C.Counter(r['bot_address'] for r in rows)
print("unique bots",len(bots))
for b,n in bots.most_common(8): print(" ",b[:12],n,f"{100*n/len(rows):.1f}%")
print("success",C.Counter(r['success'] for r in rows))
print("class x success",C.Counter((r['spam_classification'],r['success']) for r in rows))
blocks=sorted({int(r['block_number']) for r in rows}); print("blocks",len(blocks),blocks[0],blocks[-1],"span",blocks[-1]-blocks[0]+1)
times=sorted(r['block_time'] for r in rows); print("time",times[0],times[-1])
# distinct bots per block
bb=C.defaultdict(set)
for r in rows: bb[int(r['block_number'])].add(r['bot_address'])
d=C.Counter(len(v) for v in bb.values()); print("distinct bots per block",sorted(d.items()))
top=bots.most_common(1)[0][0]
tb=C.Counter(int(r['block_number']) for r in rows if r['bot_address']==top)
print("top bot present in blocks",len(tb),"of",len(blocks),"per-block median",S.median(tb.values()),"max",max(tb.values()))
# when did top bot start
print("top bot first block",min(tb),"last",max(tb))
ph=[b for b in blocks if b in tb]; 
# tx_index contiguity for top bot in one block
blk=max(tb,key=tb.get)
idx=sorted(int(r['tx_index']) for r in rows if r['bot_address']==top and int(r['block_number'])==blk)
print("top bot tx_index in block",blk,idx[:10],"...",idx[-5:])
g=[int(r['gas_used']) for r in rows if r['bot_address']==top]; print("top bot gas median",S.median(g),"min",min(g),"max",max(g))
print("top bot gas_limit",C.Counter(r['gas_limit'] for r in rows if r['bot_address']==top).most_common(3))
print("top bot class",C.Counter(r['spam_classification'] for r in rows if r['bot_address']==top))
fee=sum(float(r['total_gas_fee_eth']) for r in rows if r['bot_address']==top); print("top bot fees ETH",fee)
print("all fees ETH",sum(float(r['total_gas_fee_eth']) for r in rows))
# excluding top bot, rerun C_min=3 classification
for excl in (False,True):
  cl=C.Counter((r['block_number'],r['bot_address']) for r in rows if not(excl and r['bot_address']==top))
  tot=sum(cl.values()); hf=sum(n for n in cl.values() if n>=3)
  print("excl top" if excl else "all","HF%",round(100*hf/tot,1),"n",tot)
# per bot: median per-block count
for b,_ in bots.most_common(6):
  pb=C.Counter(int(r['block_number']) for r in rows if r['bot_address']==b)
  print(b[:12],"blocks",len(pb),"median/blk",S.median(pb.values()),"gas med",S.median(int(r['gas_used']) for r in rows if r['bot_address']==b))
