"""Check native installer against measurements of the actual CONNECT renderer."""
from pathlib import Path
import json,re,hashlib
root=Path(__file__).resolve().parents[1]
expected_font='0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7'
assert hashlib.sha256((root/'installer/assets/Montserrat.ttf').read_bytes()).hexdigest()==expected_font
geometry=(root/'src/setup_geometry.h').read_text(encoding='utf-8-sig')
values=re.findall(r'D2D1::RectF\(([^)]*)\)',geometry)
ordered=[]
for language in ['en','ru']:
 data=json.loads((root/f'installer/reference/settings-{language}.json').read_text())
 for v in data.values():ordered.append([v['x'],v['y'],v['x']+v['width'],v['y']+v['height']])
assert len(values)==len(ordered)
for native,reference in zip(values,ordered):assert all(abs(float(a.strip().rstrip('f'))-b)<1e-5 for a,b in zip(native.split(','),reference))
s=(root/'src/setup_ui.h').read_text(encoding='utf-8-sig')
colors={'Bg':('202122','f2f0eb'),'Fg':('edede9','202624'),'Muted':('a3a6a5','687071'),'Line':('414344','d9d8d2'),'Button':('e3e4df','242a28'),'Inverse':('202122','ffffff'),'Field':('292b2c','ebe9e4')}
for name,(dark,light) in colors.items():assert f'setup{name}(){{return canvas.dark?0x{dark}:0x{light};}}' in s
assert 'syncPathEdit();' not in s
assert 'DWRITE_FONT_AXIS_TAG_WEIGHT' in s and 'SetFontAxisValues' in s
assert 'DWRITE_FONT_WEIGHT_SEMI_BOLD' in s and 'DWRITE_FONT_WEIGHT_MEDIUM' in s
for forbidden in ['Язык и оформление будет','Язык и оформление будут','Language and appearance will']:
 assert forbidden not in s
assert "<<(setupEnglish?1:0)" in (root/'src/setup_options.h').read_text(encoding='utf-8-sig')
print('PASS: measured CONNECT geometry for RU/EN, exact font and palette, weights, no paint-driven path layout, language transfer')

assert 'setupLineMetrics(format.Get(),size)' in s
assert 'setupLineMetrics(format.Get(),12)' in (root/'src/setup_path_edit.h').read_text()
assert 'height=int(15*scale)' in (root/'src/setup_options.h').read_text()
for language in ['en','ru']:
 finish=json.loads((root/f'installer/reference/finish-{language}.json').read_text())
 assert finish['done']['y']==312.25 and finish['title']['y']==131.75
print('PASS: CONNECT baseline metrics, path content box, full-reference finish levels')
