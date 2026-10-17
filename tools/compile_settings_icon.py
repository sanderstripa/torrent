"""Compile the approved SVG verbatim to Direct2D commands, without approximating paths."""
from pathlib import Path
import re,xml.etree.ElementTree as ET,hashlib
root=Path(__file__).resolve().parents[1]
source=root/'assets/CONNECT-settings-08.svg'
svg=ET.parse(source).getroot()
assert svg.get('viewBox')=='5 0 85 85'
commands=[]
for node in svg:
 if node.tag.endswith('path'):
  assert node.get('fill-rule')=='evenodd' and node.get('fill')=='currentColor'
  tokens=re.findall(r'[A-Za-z]|-?\d+(?:\.\d+)?',node.get('d'));i=0;opened=False
  while i<len(tokens):
   op=tokens[i];i+=1
   count={'M':2,'C':6,'A':7,'Z':0}[op];v=tokens[i:i+count];i+=count
   f=lambda n:v[n]+'f' if '.' in v[n] else v[n]+'.f'
   if op=='M':commands.append(f'sink->BeginFigure(D2D1::Point2F({f(0)},{f(1)}),D2D1_FIGURE_BEGIN_FILLED);');opened=True
   elif op=='C':commands.append(f'sink->AddBezier(D2D1::BezierSegment(D2D1::Point2F({f(0)},{f(1)}),D2D1::Point2F({f(2)},{f(3)}),D2D1::Point2F({f(4)},{f(5)})));')
   elif op=='A':commands.append(f'sink->AddArc(D2D1::ArcSegment(D2D1::Point2F({f(5)},{f(6)}),D2D1::SizeF({f(0)},{f(1)}),{f(2)},'+('D2D1_SWEEP_DIRECTION_CLOCKWISE' if v[4]=='1' else 'D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE')+','+('D2D1_ARC_SIZE_LARGE' if v[3]=='1' else 'D2D1_ARC_SIZE_SMALL')+'));')
   else:commands.append('sink->EndFigure(D2D1_FIGURE_END_CLOSED);');opened=False
  assert not opened
 elif node.tag.endswith('circle'):
  cx,cy,r=map(float,(node.get('cx'),node.get('cy'),node.get('r')))
  commands += [f'sink->BeginFigure(D2D1::Point2F({cx+r:.6f}f,{cy:.6f}f),D2D1_FIGURE_BEGIN_FILLED);',f'sink->AddArc(D2D1::ArcSegment(D2D1::Point2F({cx-r:.6f}f,{cy:.6f}f),D2D1::SizeF({r:.6f}f,{r:.6f}f),0,D2D1_SWEEP_DIRECTION_CLOCKWISE,D2D1_ARC_SIZE_LARGE));',f'sink->AddArc(D2D1::ArcSegment(D2D1::Point2F({cx+r:.6f}f,{cy:.6f}f),D2D1::SizeF({r:.6f}f,{r:.6f}f),0,D2D1_SWEEP_DIRECTION_CLOCKWISE,D2D1_ARC_SIZE_LARGE));','sink->EndFigure(D2D1_FIGURE_END_CLOSED);']
 else:raise ValueError('Unrecognized approved SVG element')
text='// Generated from CONNECT-settings-08.svg; do not edit geometry.\n// SHA256 '+hashlib.sha256(source.read_bytes()).hexdigest()+'\n'+ '\n'.join(commands)+'\n'
target=root/'assets/settings08.inc'
import sys
if '--check' in sys.argv:assert target.read_text()==text,'Regenerate settings08.inc from approved SVG'
else:target.write_text(text,encoding='utf-8')
print('PASS: approved settings SVG exact geometry')
