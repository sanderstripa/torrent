"""Render our own vector paths to multi-resolution PNG/ICO, using only Python stdlib."""
import math, re, struct, zlib
from pathlib import Path
import xml.etree.ElementTree as ET
ROOT = Path(__file__).resolve().parents[1]
svg = ET.parse(ROOT / 'assets/icon.svg').getroot()
def polygon(data):
    tokens = re.findall(r'[MLQZ]|-?\d+(?:\.\d+)?', data)
    points, i, current = [], 0, (0, 0)
    while i < len(tokens):
        command = tokens[i]; i += 1
        if command in ('M', 'L'):
            current = (float(tokens[i]), float(tokens[i+1])); i += 2; points.append(current)
        elif command == 'Q':
            control = (float(tokens[i]), float(tokens[i+1]))
            end = (float(tokens[i+2]), float(tokens[i+3])); i += 4
            start = current
            for step in range(1, 25):
                t = step / 24
                points.append(tuple((1-t)**2*start[j]+2*(1-t)*t*control[j]+t*t*end[j] for j in (0,1)))
            current = end
    return points
def inside(points, x, y):
    result = False
    for a,b in zip(points, points[1:]+points[:1]):
        if (a[1] > y) != (b[1] > y) and x < (b[0]-a[0])*(y-a[1])/(b[1]-a[1])+a[0]: result = not result
    return result
def rgb(value):
    if len(value)==4: value='#'+''.join(c*2 for c in value[1:])
    return tuple(int(value[i:i+2],16) for i in (1,3,5))
gradients = {}
for node in svg.iter():
    if node.tag.endswith('linearGradient'):
        stops = [(float(s.get('offset','0')), rgb(s.get('stop-color'))) for s in node]
        gradients[node.get('id')] = (float(node.get('x2','1')), float(node.get('y2','0')), stops)
paths = [(polygon(node.get('d')),node.get('fill')) for node in svg if node.tag.endswith('path') and node.get('fill') != 'none']
def gradient(name,x,y):
    gx,gy,stops=gradients[name]; t=max(0,min(1,(x*gx+y*gy)/(gx*gx+gy*gy)))
    for (a,ca),(b,cb) in zip(stops,stops[1:]):
        if t <= b:
            f=(t-a)/(b-a); return tuple(round(ca[j]*(1-f)+cb[j]*f) for j in (0,1,2))
    return stops[-1][1]
def chunk(name, data): return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
def render(n):
    raw=bytearray()
    for row in range(n):
        raw.append(0)
        for column in range(n):
            samples=[]
            for sy,sx in ((.25,.25),(.25,.75),(.75,.25),(.75,.75)):
                x=(column+sx)*256/n; y=(row+sy)*256/n
                dx=max(66-x,0,x-190);dy=max(66-y,0,y-190)
                c=(*gradient('tile',x/256,y/256),255) if 12<=x<=244 and 12<=y<=244 and dx*dx+dy*dy<=54**2 else (0,0,0,0)
                for poly,fill in paths:
                    if inside(poly,x,y): c=(*gradient(fill[5:-1],x/256,y/256),255)
                samples.append(c)
            raw.extend(round(sum(c[j] for c in samples)/4) for j in range(4))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',n,n,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw,9))+chunk(b'IEND',b'')
sizes=(16,24,32,48,64,128,256)
images=[render(n) for n in sizes]
offset=6+16*len(sizes); entries=[]
for n,blob in zip(sizes,images):
    entries.append(struct.pack('<BBBBHHII',n if n<256 else 0,n if n<256 else 0,0,0,1,32,len(blob),offset));offset+=len(blob)
(ROOT/'assets/app.ico').write_bytes(struct.pack('<HHH',0,1,len(images))+b''.join(entries)+b''.join(images))
(ROOT/'assets/icon-preview.png').write_bytes(images[-1])
print('Generated app.ico with',len(images),'sizes')
