"""Compile CONNECT's exact lucide-vue-next 0.468.0 icon nodes to Direct2D."""
from pathlib import Path
import re,json,hashlib,sys
root=Path(__file__).resolve().parents[1]
base=root/'assets/connect-theme'
lines=['// Generated from the original lucide-vue-next 0.468.0 icon nodes. Do not edit.']
def f(x):return f'{float(x):.6f}f'
def pt(x,y):return f'D2D1::Point2F({f(x)},{f(y)})'
for index,name in enumerate(('monitor','sun','moon')):
 data=(base/(name+'.js')).read_bytes()
 literal=re.search(r'createLucideIcon\("[^"]+", (\[.*\])\);',data.decode(),re.S)[1]
 nodes=json.loads(re.sub(r'([,{]\s*)([A-Za-z][A-Za-z0-9]*)(\s*:)',r'\1"\2"\3',literal))
 lines+=['// SHA256 '+hashlib.sha256(data).hexdigest(),f'case {index}:']
 def begin(x,y):lines.append(f'sink->BeginFigure({pt(x,y)},D2D1_FIGURE_BEGIN_HOLLOW);')
 def line(x,y):lines.append(f'sink->AddLine({pt(x,y)});')
 def arc(x,y,rx,ry,rotation,large,sweep):lines.append(f'sink->AddArc(D2D1::ArcSegment({pt(x,y)},D2D1::SizeF({f(rx)},{f(ry)}),{f(rotation)},'+('D2D1_SWEEP_DIRECTION_CLOCKWISE' if sweep else 'D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE')+','+('D2D1_ARC_SIZE_LARGE' if large else 'D2D1_ARC_SIZE_SMALL')+'));')
 def end(closed=False):lines.append('sink->EndFigure(D2D1_FIGURE_END_'+('CLOSED' if closed else 'OPEN')+');')
 for tag,p in nodes:
  if tag=='rect':
   x,y,w,h,r=map(float,(p['x'],p['y'],p['width'],p['height'],p['rx']))
   begin(x+r,y);line(x+w-r,y);arc(x+w,y+r,r,r,0,0,1);line(x+w,y+h-r);arc(x+w-r,y+h,r,r,0,0,1);line(x+r,y+h);arc(x,y+h-r,r,r,0,0,1);line(x,y+r);arc(x+r,y,r,r,0,0,1);end(True)
  elif tag=='line':begin(p['x1'],p['y1']);line(p['x2'],p['y2']);end()
  elif tag=='circle':
   x,y,r=map(float,(p['cx'],p['cy'],p['r']));begin(x+r,y);arc(x-r,y,r,r,0,1,1);arc(x+r,y,r,r,0,1,1);end(True)
  elif tag=='path':
   tokens=re.findall(r'[A-Za-z]|-?\d+(?:\.\d+)?',p['d']);i=0;x=y=0.;op='';opened=False
   while i<len(tokens):
    if tokens[i].isalpha():op=tokens[i];i+=1
    c=op.upper();relative=op.islower()
    if c=='Z':end(True);opened=False;continue
    n={'M':2,'L':2,'H':1,'V':1,'A':7}[c];v=list(map(float,tokens[i:i+n]));i+=n
    if c in ('M','L'):
     x,y=(x+v[0],y+v[1]) if relative else v
     if c=='M':begin(x,y);opened=True;op='l' if relative else 'L'
     else:line(x,y)
    elif c=='H':x=x+v[0] if relative else v[0];line(x,y)
    elif c=='V':y=y+v[0] if relative else v[0];line(x,y)
    else:
     x,y=(x+v[5],y+v[6]) if relative else (v[5],v[6]);arc(x,y,*v[:5])
   if opened:end()
  else:raise ValueError(tag)
 lines.append('break;')
out='\n'.join(lines)+'\n';target=base/'theme-icons.inc'
if '--check' in sys.argv:assert target.read_text()==out,'Regenerate exact theme geometry'
else:target.write_text(out,encoding='utf-8')
print('PASS: exact CONNECT Monitor/Sun/Moon geometry from lucide-vue-next 0.468.0')
