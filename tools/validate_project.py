"""Offline resource and package validation; does not substitute for C++ compilation."""
import json, struct, zlib
import hashlib
from pathlib import Path
import xml.etree.ElementTree as ET
root=Path(__file__).resolve().parents[1]
font=(root/'assets/fonts/Montserrat.ttf').read_bytes()
assert font[:4]==b'\x00\x01\x00\x00' and len(font)>100000
assert 'SIL OPEN FONT LICENSE' in (root/'assets/fonts/OFL.txt').read_text()
json.loads((root/'vcpkg.json').read_text())
ET.parse(root/'assets/app.manifest')
ico=(root/'assets/app.ico').read_bytes()
reserved,kind,count=struct.unpack_from('<HHH',ico)
assert (reserved,kind,count)==(0,1,10)
for i in range(count):
    w,h,_,_,planes,bpp,length,offset=struct.unpack_from('<BBBBHHII',ico,6+16*i)
    blob=ico[offset:offset+length];assert len(blob)==length and blob.startswith(b'\x89PNG\r\n\x1a\n')
    pos=8; compressed=b''
    while pos<len(blob):
        n=struct.unpack_from('>I',blob,pos)[0];name=blob[pos+4:pos+8];data=blob[pos+8:pos+8+n]
        assert zlib.crc32(name+data)&0xffffffff==struct.unpack_from('>I',blob,pos+8+n)[0]
        if name==b'IDAT': compressed+=data
        pos+=12+n
    filtered=zlib.decompress(compressed);size=w or 256;stride=size*4
    assert len(filtered)==(stride+1)*(h or 256)
    rows=[];previous=bytearray(stride)
    for y in range(size):
        mode=filtered[y*(stride+1)];row=bytearray(filtered[y*(stride+1)+1:(y+1)*(stride+1)])
        for x in range(stride):
            a=row[x-4] if x>=4 else 0;b=previous[x];c=previous[x-4] if x>=4 else 0
            if mode==1: prediction=a
            elif mode==2: prediction=b
            elif mode==3: prediction=(a+b)//2
            elif mode==4:
                estimate=a+b-c;pa,pb,pc=abs(estimate-a),abs(estimate-b),abs(estimate-c)
                prediction=a if pa<=pb and pa<=pc else b if pb<=pc else c
            elif mode==0: prediction=0
            else: raise AssertionError('Unsupported PNG row filter')
            row[x]=(row[x]+prediction)&255
        rows.append(row);previous=row
    assert rows[size//2][(size//2)*4+3]>=240, 'Graphite tile must remain opaque'
    assert all(rows[y][x*4+3]==0 for x,y in [(0,0),(size-1,0),(0,size-1),(size-1,size-1)]), 'Exterior corners must be transparent'
    visible=[(x,y) for y in range(size) for x in range(size) if rows[y][x*4+3]>128]
    assert visible and max(y for x,y in visible)-min(y for x,y in visible)+1>=int(size*.95), 'Squircle has excessive padding'
    assert planes==1 and bpp==32
for name in ('src/main.cpp','src/engine.cpp','src/engine.h','installer/Torrent.nsi','.github/workflows/windows.yml'):
    assert (root/name).stat().st_size>0
print('PASS: JSON, XML, 10 ICO frames, PNG CRC/decompression, transparency, tight framing, required sources')
