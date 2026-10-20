"""Package the redrawn master into Windows sizes. Regeneration requires Pillow."""
import struct,zlib
from pathlib import Path
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]
SIZES=(16,20,24,32,40,48,64,96,128,256)
def chunk(name,data):return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
def png(image):
 n=image.width;pixels=image.tobytes();raw=b''.join(b'\0'+pixels[y*n*4:(y+1)*n*4] for y in range(n))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',n,n,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw,9))+chunk(b'IEND',b'')
master=Image.open(ROOT/'assets/icon-master.png').convert('RGBA')
# Optical bounds exclude the effectively invisible alpha fringe; preserve the original master.
bounds=master.getchannel('A').point(lambda a:255 if a>=16 else 0).getbbox()
assert bounds
art=master.crop(bounds);side=max(art.size)
square=Image.new('RGBA',(side,side));square.paste(art,((side-art.width)//2,(side-art.height)//2))
images=[]
for size in SIZES:
 frame=square.resize((size,size),Image.Resampling.LANCZOS);images.append(png(frame))
 (ROOT/f'assets/icon-{size}.png').write_bytes(images[-1])
offset=6+16*len(SIZES);entries=[]
for size,data in zip(SIZES,images):
 entries.append(struct.pack('<BBBBHHII',size%256,size%256,0,0,1,32,len(data),offset));offset+=len(data)
(ROOT/'assets/app.ico').write_bytes(struct.pack('<HHH',0,1,len(SIZES))+b''.join(entries)+b''.join(images))
(ROOT/'assets/icon-preview.png').write_bytes(images[-1])
print('Generated reference icon:',SIZES)
