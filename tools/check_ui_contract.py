"""Protect Torrent's approved palette and embedded Montserrat while unifying controls."""
from pathlib import Path
import hashlib,re
root=Path(__file__).resolve().parents[1]
s=(root/'src/visual.h').read_text(encoding='utf-8')
expected={'fg':('eeeeef','242628'),'muted':('96979d','73767b'),'card':('111215','fcfcfd'),'lineColor':('292a2e','e6e7e9'),'background':('08090b','ffffff'),'selected':('202125','ededef'),'track':('303136','e6e7e9')}
for role,(dark,light) in expected.items():
 assert re.search(r'UINT '+role+r'\(\)const\{return dark\?0x'+dark+':0x'+light+r';\}',s),role
assert hashlib.sha256((root/'assets/fonts/Montserrat.ttf').read_bytes()).hexdigest()=='3e8abe50c44c82e2242e97d1ec8c0d385c4890cdc50447bcdb8605c81a38cfb2'
main=(root/'src/main.cpp').read_text(encoding='utf-8')
assert 'int(940*GetDpiForSystem()/96.f),int(720*GetDpiForSystem()/96.f)' in main
assert 'WS_OVERLAPPEDWINDOW' not in main
assert 'WS_MAXIMIZEBOX' not in main
assert 'prefs.dark==2' in main and 'WM_SETTINGCHANGE' in main
print('PASS: Soulu palette, embedded Montserrat, expanded dimensions; custom shell and System theme')
