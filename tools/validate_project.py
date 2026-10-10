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
ET.parse(root/'assets/icon.svg');ET.parse(root/'assets/app.manifest')
ico=(root/'assets/app.ico').read_bytes()
reserved,kind,count=struct.unpack_from('<HHH',ico)
assert (reserved,kind,count)==(0,1,7)
for i in range(count):
    w,h,_,_,planes,bpp,length,offset=struct.unpack_from('<BBBBHHII',ico,6+16*i)
    blob=ico[offset:offset+length];assert len(blob)==length and blob.startswith(b'\x89PNG\r\n\x1a\n')
    pos=8; compressed=b''
    while pos<len(blob):
        n=struct.unpack_from('>I',blob,pos)[0];name=blob[pos+4:pos+8];data=blob[pos+8:pos+8+n]
        assert zlib.crc32(name+data)&0xffffffff==struct.unpack_from('>I',blob,pos+8+n)[0]
        if name==b'IDAT': compressed+=data
        pos+=12+n
    raw=zlib.decompress(compressed);size=w or 256;stride=size*4+1
    assert len(raw)==stride*(h or 256)
    assert all(raw[y*stride]==0 for y in range(size))
    assert raw[4]==0, 'Icon corner must be transparent'
    assert raw[(size//2)*stride+1+(size//2)*4+3]==0, 'Ribbon opening must be transparent'
    visible=[(x,y) for y in range(size) for x in range(size) if raw[y*stride+1+x*4+3]>0]
    assert visible and max(y for x,y in visible)-min(y for x,y in visible)+1>=size*.98, 'Ribbon must fill the icon height'
    assert all(raw[y*stride+1+x*4]<=62 for x,y in visible), 'White framing must be absent'
    assert planes==1 and bpp==32
for name in ('src/main.cpp','src/engine.cpp','src/engine.h','installer/Torrent.nsi','.github/workflows/windows.yml'):
    assert (root/name).stat().st_size>0
print('PASS: JSON, XML, 7 ICO frames, PNG CRC/decompression, transparency, tight framing, required sources')
