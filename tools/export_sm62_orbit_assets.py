"""Export lossless firmware RLE, never baked scenes or star positions."""
from pathlib import Path
import re,struct,sys,hashlib,json
f=Path(sys.argv[1]);root=Path(__file__).resolve().parents[1]
source=f/'keyboards/ergohaven/sm30/anim.c';text=source.read_text();data=bytearray();records=[]
for i in range(16):
    for kind in ['anim','flame']:
        name=f'{kind}_{i:02}'
        body=re.search(r'uint8_t '+name+r'_map\[\] = \{(.*?)\};',text,re.S)[1]
        raw=bytes(int(v.strip(),0) for v in body.split(',') if v.strip())
        descriptor=re.search(r'const lv_img_dsc_t '+name+r' = \{(.*?)\};',text,re.S)[1]
        assert int(re.search(r'\.data_size = (\d+)',descriptor)[1])==len(raw)
        assert sum(x>>4 for x in raw[52:])==raw[50]*raw[51]
        data+=struct.pack('<I',len(raw))+raw
        records.append({'name':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
p=root/'assets/sm62/orbit-rle.bin';p.write_bytes(data)
(root/'assets/sm62/orbit-rle-provenance.json').write_text(json.dumps({'source':'keyboards/ergohaven/sm30/anim.c','source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'sha256':hashlib.sha256(data).hexdigest(),'records':records},indent=2)+'\n')
print('Exported',len(records),'unmodified RLE assets,',len(data),'bytes')
