"""Protect Torrent's approved palette and embedded Montserrat while unifying controls."""
from pathlib import Path
import hashlib,re
root=Path(__file__).resolve().parents[1]
s=(root/'src/visual.h').read_text(encoding='utf-8')
expected={'fg':('f2f2f1','17191c'),'muted':('94989c','777c81'),'card':('101010','fafafa'),'lineColor':('303234','dadbd9'),'background':('0c0c0c','f5f5f4'),'selected':('222324','e8e9e7'),'track':('303234','e0e1de')}
for role,(dark,light) in expected.items():
 assert re.search(r'UINT '+role+r'\(\)const\{return dark\?0x'+dark+':0x'+light+r';\}',s),role
assert hashlib.sha256((root/'assets/fonts/Montserrat.ttf').read_bytes()).hexdigest()=='3e8abe50c44c82e2242e97d1ec8c0d385c4890cdc50447bcdb8605c81a38cfb2'
main=(root/'src/main.cpp').read_text(encoding='utf-8')
assert 'int(840*GetDpiForSystem()/96.f),int(610*GetDpiForSystem()/96.f)' in main
assert 'WS_OVERLAPPEDWINDOW' not in main
assert 'WS_MAXIMIZEBOX' not in main
assert 'prefs.dark==2' in main and 'WM_SETTINGCHANGE' in main
print('PASS: unchanged palette, Montserrat, outer dimensions; custom shell and System theme')
