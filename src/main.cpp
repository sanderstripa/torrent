#include "engine.h"
#include "visual.h"
#include "popup.h"
#include <windows.h>
#include <windowsx.h>
#include <commctrl.h>
#include <commdlg.h>
#include <shellapi.h>
#include <shlobj.h>
#include <d2d1.h>
#include <dwrite.h>
#include <wrl/client.h>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <fstream>
#include <memory>
#include <algorithm>
#include <sstream>
#include <stdexcept>
#include <cwctype>
using Microsoft::WRL::ComPtr;
static HINSTANCE instance; static HWND mainWindow; static std::unique_ptr<Engine> engine;
static std::filesystem::path dataRoot;
static std::filesystem::path knownFolder(REFKNOWNFOLDERID id) {PWSTR value=nullptr; if(FAILED(SHGetKnownFolderPath(id,KF_FLAG_DONT_VERIFY,nullptr,&value)))throw std::runtime_error("Cannot resolve Windows user folder");std::filesystem::path result(value);CoTaskMemFree(value);return result;}
struct Preferences { std::wstring folder; int action=0, dark=0, english=0; } prefs;
static int scroll=0; static bool closing=false;
static bool smokeTesting=false;static bool designPreview=false,interactionTesting=false;
static void stage(char const* value) {if(smokeTesting)std::ofstream("Torrent-startup-stages.txt",std::ios::app)<<value<<'\n';}
static std::wstring tr(wchar_t const* ru,wchar_t const* en) {return prefs.english?en:ru;}
static void error(std::string const& message) {MessageBoxW(mainWindow,wide(message).c_str(),L"Torrent",MB_OK|MB_ICONERROR);}
static void savePrefs() { std::ofstream f(dataRoot/L"settings.txt",std::ios::binary); f<<utf8(prefs.folder)<<'\n'<<prefs.action<<' '<<prefs.dark<<' '<<prefs.english; }
static std::wstring sizeText(std::int64_t size) { wchar_t b[64];double divisor=1024;std::wstring unit=tr(L"КБ",L"KB");if(size>=1024LL*1024*1024){divisor=1024LL*1024*1024;unit=tr(L"ГБ",L"GB");}else if(size>=1024*1024){divisor=1024*1024;unit=tr(L"МБ",L"MB");}swprintf_s(b,L"%.1f",double(size)/divisor);std::wstring value=b;if(!prefs.english)std::replace(value.begin(),value.end(),L'.',L',');return value+L" "+unit; }
static HWND control(HWND parent,wchar_t const* cls,std::wstring const& text,DWORD style,int x,int y,int w,int h,int id) { HWND c=CreateWindowExW(0,cls,text.c_str(),WS_CHILD|WS_VISIBLE|style,x,y,w,h,parent,HMENU(INT_PTR(id)),instance,nullptr); SendMessageW(c,WM_SETFONT,WPARAM(GetStockObject(DEFAULT_GUI_FONT)),TRUE); return c; }
static std::wstring textOf(HWND c) {int n=GetWindowTextLengthW(c); std::wstring t(n+1,0); GetWindowTextW(c,t.data(),n+1); t.resize(n); return t;}
static bool browse(HWND owner,std::wstring& folder) {ComPtr<IFileDialog> d; if(FAILED(CoCreateInstance(CLSID_FileOpenDialog,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&d)))) return false; d->SetOptions(FOS_PICKFOLDERS|FOS_FORCEFILESYSTEM); if(FAILED(d->Show(owner))) return false; ComPtr<IShellItem> item; d->GetResult(&item); PWSTR p=nullptr; if(FAILED(item->GetDisplayName(SIGDN_FILESYSPATH,&p))) return false; folder=p; CoTaskMemFree(p); return true;}

struct Dialog {
 int kind; bool done=false,accepted=false; lt::torrent_handle handle; std::wstring result;
 HWND window=nullptr;std::vector<int> priorities,remembered;std::vector<std::wstring> names,sizes;
 visual::Canvas canvas;int offset=0,hover=-1,focused=1;Preferences draft;HFONT font=nullptr;HBRUSH background=nullptr;bool fixture=false,folderValid=true;
};
static void fillFiles(Dialog& d) {
 auto ti=d.handle.torrent_file();if(!ti)return;auto ps=d.handle.get_file_priorities();
 bool pending=std::any_of(engine->items.begin(),engine->items.end(),[&](auto const& item){return item.handle==d.handle&&item.selectPending;});
 for(int i=0;i<ti->num_files();++i){auto index=lt::file_index_t(i);int p=i<int(ps.size())?static_cast<std::uint8_t>(ps[i]):4;if(pending&&p==0)p=4;
 d.priorities.push_back(p);d.remembered.push_back(p?p:4);d.names.push_back(std::filesystem::path(wide(ti->files().file_path(index))).filename().wstring());d.sizes.push_back(sizeText(ti->files().file_size(index)));}
}
static std::wstring priorityLabel(int p){return p==0?tr(L"Не скачивать",L"Skip"):p<=1?tr(L"Низкий",L"Low"):p>=7?tr(L"Высокий",L"High"):tr(L"Обычный",L"Normal");}
static void drawDialog(HWND w,Dialog& d){PAINTSTRUCT ps;BeginPaint(w,&ps);auto& c=d.canvas;c.dark=prefs.dark!=0;
 if(c.begin(w)){float width=c.width,height=c.height;
 c.titlebar(d.kind==2?tr(L"Файлы торрента",L"Torrent files"):d.kind==1?tr(L"Настройки",L"Settings"):tr(L"Magnet-ссылка",L"Magnet link"));
 if(d.kind==2){auto ti=d.fixture?std::shared_ptr<lt::torrent_info const>{}:d.handle.torrent_file();c.label(d.fixture?L"The Witcher S01":ti?wide(ti->name()):tr(L"Получение списка файлов…",L"Fetching file list…"),D2D1::RectF(24,50,width-24,76),17,true);
 c.label(d.fixture?tr(L"12,6 ГБ · 10 файлов",L"12.6 GB · 10 files"):ti?sizeText(ti->total_size())+L" · "+std::to_wstring(ti->num_files())+tr(L" файлов",L" files"):L"",D2D1::RectF(24,76,width-24,98),13,false,c.muted());
 c.box(D2D1::RectF(18,112,width-18,height-70),c.card(),9,c.lineColor());c.box(D2D1::RectF(18,112,width-18,143),c.dark?0x28374c:0xf4f4f5,7);
 c.check(32,120,!d.priorities.empty()&&std::all_of(d.priorities.begin(),d.priorities.end(),[](int p){return p>0;}));
 c.label(tr(L"Имя файла",L"File name"),D2D1::RectF(68,112,width-258,143),13,false,c.muted());c.label(tr(L"Размер",L"Size"),D2D1::RectF(width-242,112,width-156,143),13,false,c.muted());c.label(tr(L"Приоритет",L"Priority"),D2D1::RectF(width-148,112,width-30,143),13,false,c.muted());
 c.rt->PushAxisAlignedClip(D2D1::RectF(19,143,width-19,height-71),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 for(int i=0;i<int(d.priorities.size());++i){float y=143.f+i*42-d.offset;if(y+42<143||y>height-70)continue;
 if(d.focused==100+i)c.box(D2D1::RectF(20,y+1,width-20,y+41),c.dark?0x2c405b:0xf0f6ff,0);c.check(32,y+13,d.priorities[i]!=0);auto extension=std::filesystem::path(d.names[i]).extension().wstring();std::transform(extension.begin(),extension.end(),extension.begin(),[](wchar_t ch){return wchar_t(towlower(ch));});c.box(D2D1::RectF(70,y+12,84,y+30),c.muted(),2);if(extension==L".mkv"||extension==L".mp4"||extension==L".avi"){c.line(75,y+17,80,y+21,c.card(),1.7f);c.line(80,y+21,75,y+25,c.card(),1.7f);c.line(75,y+25,75,y+17,c.card(),1.7f);}else if(extension==L".jpg"||extension==L".png"||extension==L".jpeg"){c.line(72,y+27,76,y+21,c.card());c.line(76,y+21,79,y+25,c.card());c.line(79,y+25,81,y+22,c.card());}else {c.line(73,y+18,81,y+18,c.card(),1);c.line(73,y+22,81,y+22,c.card(),1);c.line(73,y+26,78,y+26,c.card(),1);}
 c.label(d.names[i],D2D1::RectF(98,y,width-254,y+42),14);c.label(d.sizes[i],D2D1::RectF(width-242,y,width-158,y+42),13,false,c.muted());
 int p=d.priorities[i];c.box(D2D1::RectF(width-150,y+6,width-30,y+36),c.dark?0x29394f:p>=7?0xe8f3ff:p==1?0xedf9f0:0xf6f6f7,6,c.lineColor());c.label(priorityLabel(p),D2D1::RectF(width-140,y+6,width-48,y+36),12,false,p>=7?0x087cff:p==1?0x25a66b:c.muted());c.chevron(width-47,y+18);c.line(20,y+42,width-20,y+42,c.lineColor(),1);}
 c.rt->PopAxisAlignedClip();
 }else if(d.kind==1){
 c.label(tr(L"Папка загрузок",L"Download folder"),D2D1::RectF(24,67,width-24,95),14,false,c.muted());c.box(D2D1::RectF(24,101,width-78,141),c.card(),7,c.lineColor());c.button(L"…",D2D1::RectF(width-66,101,width-24,141),false,d.hover==11);
 const std::wstring labels[]={tr(L"При добавлении",L"On adding"),tr(L"Тема",L"Theme"),tr(L"Язык",L"Language")};const std::wstring values[]={d.draft.action?tr(L"Выбрать файлы",L"Choose files"):tr(L"Начать загрузку",L"Start download"),d.draft.dark?tr(L"Тёмная",L"Dark"):tr(L"Светлая",L"Light"),d.draft.english?L"English":L"Русский"};
 for(int i=0;i<3;++i){float y=163.f+i*54;c.label(labels[i],D2D1::RectF(24,y,width-250,y+40),15);c.box(D2D1::RectF(width-250,y,width-24,y+40),c.card(),7,c.lineColor());c.label(values[i],D2D1::RectF(width-236,y,width-48,y+40),14);c.chevron(width-44,y+18);}
 if(!d.folderValid)c.label(tr(L"Укажите полный путь к папке",L"Enter an absolute folder path"),D2D1::RectF(24,141,width-24,160),12,false,0xc95050);
 }else {c.label(tr(L"Вставьте ссылку",L"Paste a magnet link"),D2D1::RectF(24,62,width-24,94),14,false,c.muted());c.box(D2D1::RectF(24,100,width-24,142),c.card(),7,c.lineColor());}
 if(d.kind!=1){if(GetFocus()==w&&(d.focused==1||d.focused==2)){float x=d.focused==1?width-131:width-243;c.box(D2D1::RectF(x,height-57,x+110,height-15),c.dark?0x233955:0xe5f0ff,11,0x8ebcff);}
 c.button(tr(L"Отмена",L"Cancel"),D2D1::RectF(width-240,height-54,width-136,height-18),false,d.hover==2);c.button(tr(L"Применить",L"Apply"),D2D1::RectF(width-128,height-54,width-24,height-18),true,d.hover==1);}
 c.end();}EndPaint(w,&ps);}
static int dialogHit(Dialog& d,float x,float y){auto& c=d.canvas;if(d.kind!=1&&visual::hit(x,y,D2D1::RectF(c.width-240,c.height-54,c.width-136,c.height-18)))return 2;if(d.kind!=1&&visual::hit(x,y,D2D1::RectF(c.width-128,c.height-54,c.width-24,c.height-18)))return 1;if(x>c.width-46&&y<50)return 2;if(d.kind==1){if(y>=101&&y<=141&&x>c.width-66)return 11;for(int i=0;i<3;++i)if(y>=163+i*54&&y<203+i*54&&x>=c.width-250)return 12+i;}return -1;}
static void liveSettings(HWND w,Dialog& d){
 auto folder=textOf(GetDlgItem(w,10));d.folderValid=!folder.empty()&&folder.find_first_of(L"\r\n")==std::wstring::npos&&std::filesystem::path(folder).is_absolute();
 if(d.folderValid)d.draft.folder=folder;else d.draft.folder=prefs.folder;
 prefs=d.draft;savePrefs();DeleteObject(d.background);d.background=CreateSolidBrush(prefs.dark?RGB(32,44,62):RGB(255,255,255));visual::frame(mainWindow,prefs.dark!=0);visual::frame(w,prefs.dark!=0,true);SetWindowTextW(w,tr(L"Настройки",L"Settings").c_str());InvalidateRect(GetDlgItem(w,10),nullptr,TRUE);InvalidateRect(w,nullptr,FALSE);InvalidateRect(mainWindow,nullptr,FALSE);
}
static LRESULT CALLBACK editProc(HWND w,UINT m,WPARAM wp,LPARAM lp,UINT_PTR,DWORD_PTR){if(m==WM_CONTEXTMENU){HWND owner=GetParent(w);DWORD start=0,end=0;SendMessageW(w,EM_GETSEL,WPARAM(&start),LPARAM(&end));POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};if(point.x==-1&&point.y==-1){RECT r;GetWindowRect(w,&r);point={r.left+12,r.bottom};}int id=visual::menu(owner,point,{{1,tr(L"Вырезать",L"Cut"),false,start!=end},{2,tr(L"Копировать",L"Copy"),false,start!=end},{3,tr(L"Вставить",L"Paste"),false,IsClipboardFormatAvailable(CF_UNICODETEXT)!=0},{4,tr(L"Выделить всё",L"Select all")}},prefs.dark!=0);if(id==1)SendMessageW(w,WM_CUT,0,0);if(id==2)SendMessageW(w,WM_COPY,0,0);if(id==3)SendMessageW(w,WM_PASTE,0,0);if(id==4)SendMessageW(w,EM_SETSEL,0,-1);SetFocus(w);return 0;}return DefSubclassProc(w,m,wp,lp);}
static void acceptDialog(HWND w,Dialog& d){if(d.kind==1){liveSettings(w,d);return;}

 if(d.kind==0)d.result=textOf(GetDlgItem(w,10));

 if(d.kind==2){if(!d.handle.torrent_file())return;std::vector<lt::download_priority_t> p;for(int value:d.priorities)p.push_back(lt::download_priority_t(static_cast<std::uint8_t>(value)));d.handle.prioritize_files(p);for(auto& item:engine->items)if(item.handle==d.handle&&item.selectPending){item.selectPending=false;item.handle.unset_flags(lt::torrent_flags::default_dont_download);item.handle.set_flags(lt::torrent_flags::auto_managed);item.handle.resume();}engine->save();}
 d.accepted=true;DestroyWindow(w);
}
static LRESULT CALLBACK dialogProc(HWND w,UINT m,WPARAM wp,LPARAM lp){auto d=reinterpret_cast<Dialog*>(GetWindowLongPtrW(w,GWLP_USERDATA));if(m==WM_NCCREATE){d=static_cast<Dialog*>(reinterpret_cast<CREATESTRUCTW*>(lp)->lpCreateParams);SetWindowLongPtrW(w,GWLP_USERDATA,LONG_PTR(d));d->window=w;}if(!d)return DefWindowProcW(w,m,wp,lp);
 switch(m){case WM_CREATE:{d->draft=prefs;visual::frame(w,prefs.dark!=0,true);float scale=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);int width=int(r.right/scale);d->font=CreateFontW(-int(15*scale),0,0,0,FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,L"Segoe UI Variable Text");d->background=CreateSolidBrush(prefs.dark?RGB(32,44,62):RGB(255,255,255));
 if(d->kind!=2){HWND edit=control(w,L"EDIT",d->kind==1?prefs.folder:L"",ES_AUTOHSCROLL|WS_TABSTOP,int(34*scale),int(111*scale),int((width-(d->kind==1?122:68))*scale),int(22*scale),10);SendMessageW(edit,WM_SETFONT,WPARAM(d->font),TRUE);SetWindowSubclass(edit,editProc,1,0);SetFocus(edit);d->focused=10;}else if(!d->fixture){fillFiles(*d);if(!d->handle.torrent_file())SetTimer(w,2,500,nullptr);}if(designPreview)SetTimer(w,99,250,nullptr);return 0;}
 case WM_NCCALCSIZE:if(wp)return 0;break;
 case WM_NCPAINT:return 0;case WM_NCACTIVATE:return TRUE;
 case WM_PAINT:drawDialog(w,*d);return 0;case WM_ERASEBKGND:return 1;
 case WM_CTLCOLOREDIT:{auto dc=HDC(wp);SetTextColor(dc,prefs.dark?RGB(232,238,248):RGB(17,24,39));SetBkColor(dc,prefs.dark?RGB(32,44,62):RGB(255,255,255));return LRESULT(d->background);}
 case WM_NCHITTEST:{POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(w,&point);float scale=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);if(point.y<52*scale&&point.x<r.right-50*scale)return HTCAPTION;return HTCLIENT;}
 case WM_GETMINMAXINFO:{RECT r;GetWindowRect(w,&r);auto info=reinterpret_cast<MINMAXINFO*>(lp);info->ptMinTrackSize={r.right-r.left,r.bottom-r.top};info->ptMaxTrackSize=info->ptMinTrackSize;return 0;}
 case WM_MOUSEMOVE:{float scale=GetDpiForWindow(w)/96.f;int h=dialogHit(*d,GET_X_LPARAM(lp)/scale,GET_Y_LPARAM(lp)/scale);if(h!=d->hover){d->hover=h;InvalidateRect(w,nullptr,FALSE);}SetCursor(LoadCursorW(nullptr,h>=0?IDC_HAND:IDC_ARROW));return 0;}
 case WM_LBUTTONUP:{float scale=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/scale,y=GET_Y_LPARAM(lp)/scale;int id=dialogHit(*d,x,y);
 if(id==1)acceptDialog(w,*d);else if(id==2)DestroyWindow(w);else if(id==11){auto folder=textOf(GetDlgItem(w,10));if(browse(w,folder))SetWindowTextW(GetDlgItem(w,10),folder.c_str());}else if(id>=12&&id<=14){std::wstring values[2];int selected=0;if(id==12){values[0]=tr(L"Начать загрузку",L"Start download");values[1]=tr(L"Выбрать файлы",L"Choose files");selected=d->draft.action;}if(id==13){values[0]=tr(L"Светлая",L"Light");values[1]=tr(L"Тёмная",L"Dark");selected=d->draft.dark;}if(id==14){values[0]=L"Русский";values[1]=L"English";selected=d->draft.english;}POINT point{int((d->canvas.width-250)*scale),int((203+(id-12)*54)*scale)};ClientToScreen(w,&point);int choice=visual::menu(w,point,{{1,values[0],selected==0},{2,values[1],selected==1}},prefs.dark!=0);if(choice){if(id==12)d->draft.action=choice-1;if(id==13)d->draft.dark=choice-1;if(id==14)d->draft.english=choice-1;liveSettings(w,*d);}}

 else if(d->kind==2){if(y>=112&&y<143&&x<60){bool all=std::all_of(d->priorities.begin(),d->priorities.end(),[](int p){return p>0;});for(int i=0;i<int(d->priorities.size());++i){int& p=d->priorities[i];if(all){d->remembered[i]=p;p=0;}else if(!p)p=d->remembered[i];}}else if(y>=143&&y<d->canvas.height-70){int row=int(y-143+d->offset)/42;if(row>=0&&row<int(d->priorities.size())){d->focused=100+row;if(x<60){if(d->priorities[row]){d->remembered[row]=d->priorities[row];d->priorities[row]=0;}else d->priorities[row]=d->remembered[row];}else if(x>d->canvas.width-150){int values[]={0,1,4,7};std::vector<visual::MenuItem> entries;for(int i=0;i<4;++i)entries.push_back({i+1,priorityLabel(values[i]),d->priorities[row]==values[i]});POINT point{int((d->canvas.width-150)*scale),int((143+row*42-d->offset+36)*scale)};ClientToScreen(w,&point);int choice=visual::menu(w,point,std::move(entries),prefs.dark!=0);if(choice){if(d->priorities[row])d->remembered[row]=d->priorities[row];d->priorities[row]=values[choice-1];}}}}}InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_MOUSEWHEEL:if(d->kind==2){d->offset=std::clamp(d->offset-GET_WHEEL_DELTA_WPARAM(wp)/3,0,std::max(0,int(d->priorities.size())*42-int(d->canvas.height-213)));InvalidateRect(w,nullptr,FALSE);}return 0;
 case WM_COMMAND:if(d->kind==1&&LOWORD(wp)==10&&HIWORD(wp)==EN_CHANGE){liveSettings(w,*d);return 0;}if(LOWORD(wp)==1)acceptDialog(w,*d);if(LOWORD(wp)==2)DestroyWindow(w);return 0;
 case WM_TIMER:if(wp==99){KillTimer(w,99);visual::capture(w,d->kind==2?L"files.png":L"settings.png");DestroyWindow(w);return 0;}if(d->kind==2&&d->handle.torrent_file()){fillFiles(*d);KillTimer(w,2);InvalidateRect(w,nullptr,FALSE);}return 0;
 case WM_CLOSE:DestroyWindow(w);return 0;case WM_DESTROY:d->done=true;DeleteObject(d->font);DeleteObject(d->background);return 0;
 }return DefWindowProcW(w,m,wp,lp);}
static bool dialog(Dialog& d){EnableWindow(mainWindow,FALSE);float scale=GetDpiForWindow(mainWindow)/96.f;int width=d.kind==2?580:490,height=d.kind==2?std::clamp(213+(d.fixture?5:d.handle.torrent_file()?d.handle.torrent_file()->num_files():5)*42,300,590):d.kind==1?335:215;RECT r;GetWindowRect(mainWindow,&r);HWND w=CreateWindowExW(0,L"TorrentDialog",d.kind==2?tr(L"Файлы торрента",L"Torrent files").c_str():d.kind==1?tr(L"Настройки",L"Settings").c_str():L"Magnet",WS_POPUP|WS_THICKFRAME|WS_SYSMENU|WS_CLIPCHILDREN,r.left+(r.right-r.left-int(width*scale))/2,r.top+(r.bottom-r.top-int(height*scale))/2,int(width*scale),int(height*scale),mainWindow,nullptr,instance,&d);ShowWindow(w,SW_SHOW);MSG m;while(!d.done&&GetMessageW(&m,nullptr,0,0)>0){if(m.message==WM_KEYDOWN&&m.wParam==VK_TAB){std::vector<int> ids=d.kind==1?std::vector<int>{10,11,12,13,14}:d.kind==0?std::vector<int>{10,2,1}:std::vector<int>{100,2,1};auto found=std::find(ids.begin(),ids.end(),d.focused);int at=found==ids.end()?0:int(found-ids.begin());at=(at+((GetKeyState(VK_SHIFT)&0x8000)?int(ids.size())-1:1))%int(ids.size());d.focused=ids[at];SetFocus(d.focused==10?GetDlgItem(w,10):w);InvalidateRect(w,nullptr,FALSE);continue;}
 if(m.message==WM_KEYDOWN&&d.kind==2&&d.focused>=100){int row=d.focused-100;if(m.wParam==VK_DOWN||m.wParam==VK_UP){row=std::clamp(row+(m.wParam==VK_DOWN?1:-1),0,std::max(0,int(d.priorities.size())-1));d.focused=100+row;d.offset=std::clamp(row*42-84,0,std::max(0,int(d.priorities.size())*42-int(d.canvas.height-213)));}else if(row<int(d.priorities.size())&&m.wParam==VK_SPACE){if(d.priorities[row]){d.remembered[row]=d.priorities[row];d.priorities[row]=0;}else d.priorities[row]=d.remembered[row];}else if(row<int(d.priorities.size())&&(m.wParam==VK_LEFT||m.wParam==VK_RIGHT)){int values[]={0,1,4,7};int at=0;while(at<3&&d.priorities[row]>values[at])++at;d.priorities[row]=values[(at+(m.wParam==VK_RIGHT?1:3))%4];}else {TranslateMessage(&m);DispatchMessageW(&m);}InvalidateRect(w,nullptr,FALSE);continue;}
 if(m.message==WM_KEYDOWN&&(m.wParam==VK_SPACE||m.wParam==VK_RETURN)&&d.focused!=10){if(d.focused==2)DestroyWindow(w);else if(d.focused==1)acceptDialog(w,d);else {float x=d.canvas.width-40,y=d.focused==11?120.f:183.f+(d.focused-12)*54;PostMessageW(w,WM_LBUTTONUP,0,MAKELPARAM(int(x*scale),int(y*scale)));}continue;}
 if(m.message==WM_KEYDOWN&&m.wParam==VK_ESCAPE){DestroyWindow(w);continue;}if(m.message==WM_KEYDOWN&&m.wParam==VK_RETURN){acceptDialog(w,d);continue;}TranslateMessage(&m);DispatchMessageW(&m);}EnableWindow(mainWindow,TRUE);SetForegroundWindow(mainWindow);InvalidateRect(mainWindow,nullptr,FALSE);return d.accepted;}
static void add(std::wstring const& source) {try {auto h=engine->add(utf8(source),utf8(prefs.folder),prefs.action==1); if(prefs.action==1&&h.torrent_file()) {for(auto& item:engine->items)if(item.handle==h)item.selectionShown=true;Dialog d{2};d.handle=h;dialog(d);} InvalidateRect(mainWindow,nullptr,FALSE);} catch(std::exception const& e) {error(e.what());}}
static void openTorrent() {wchar_t file[32768]{}; OPENFILENAMEW o{}; o.lStructSize=sizeof(o); o.hwndOwner=mainWindow; o.lpstrFilter=L"Torrent\0*.torrent\0"; o.lpstrFile=file; o.nMaxFile=32768; o.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR; if(GetOpenFileNameW(&o)) add(file);}

static visual::Canvas mainCanvas;
static int rowAt(int y){int row=(y-86+scroll)/136;return y>=86&&row>=0&&row<int(engine->items.size())&&(y-86+scroll)%136<124?row:-1;}
static void paint(HWND w){PAINTSTRUCT ps;BeginPaint(w,&ps);auto& c=mainCanvas;c.dark=prefs.dark!=0;
 if(c.begin(w)){float width=c.width;
 c.label(L"Torrent",D2D1::RectF(24,18,width-150,62),27,true);
 c.box(D2D1::RectF(width-112,22,width-74,60),c.dark?0x28374c:0xf4f4f5,9);c.line(width-100,41,width-86,41,c.muted());c.line(width-93,34,width-93,48,c.muted());
 c.box(D2D1::RectF(width-60,22,width-22,60),c.dark?0x28374c:0xf4f4f5,9);
 for(int i=0;i<3;++i){float y=33.f+i*7;c.line(width-49,y,width-33,y,c.muted());float x=i==1?width-37:width-45;c.box(D2D1::RectF(x-2,y-2,x+2,y+2),c.card(),2,c.muted());}
 if(designPreview){const wchar_t* names[]={L"Ubuntu Desktop.iso",L"Linux Mint.iso",L"The Witcher S01"};const wchar_t* details[]={L"3,2 ГБ · Загрузка",L"2,8 ГБ · Завершено · Раздача",L"12,6 ГБ · Пауза"};float progress[]={.72f,1.f,.43f};for(int i=0;i<3;++i){float y=86.f+i*136;c.box(D2D1::RectF(18,y,width-18,y+124),i==2?0xe5f0ff:c.card(),11,c.lineColor());c.label(names[i],D2D1::RectF(34,y+12,width-110,y+42),19,true);c.label(details[i],D2D1::RectF(34,y+43,width-110,y+68),15,false,c.muted());if(i==1)c.success(width-51,y+42);else c.label(std::to_wstring(int(progress[i]*100))+L"%",D2D1::RectF(width-93,y+30,width-34,y+58),18,true,0,DWRITE_TEXT_ALIGNMENT_TRAILING);c.box(D2D1::RectF(34,y+77,width-34,y+85),0xe8e9ec,4);c.box(D2D1::RectF(34,y+77,34+(width-68)*progress[i],y+85),0x328cff,4);c.label(i==0?L"Осталось 1 мин 12 сек":i==1?L"100%":L"Пауза",D2D1::RectF(width-310,y+92,width-34,y+116),14,false,c.muted(),DWRITE_TEXT_ALIGNMENT_TRAILING);}}else if(engine->items.empty()){c.label(tr(L"Добавьте первый торрент",L"Add your first torrent"),D2D1::RectF(24,c.height/2-35,width-24,c.height/2),20,true,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);c.label(tr(L"Нажмите + или перетащите сюда .torrent",L"Click + or drop a .torrent file here"),D2D1::RectF(24,c.height/2,width-24,c.height/2+32),15,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 c.rt->PushAxisAlignedClip(D2D1::RectF(0,78,width,c.height),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 for(int i=0;i<int(engine->items.size());++i){auto const& item=engine->items[i];auto s=item.handle.status();float y=86.f+i*136-scroll;if(y+124<78||y>c.height)continue;
 bool paused=(s.flags&lt::torrent_flags::paused)&&!(s.flags&lt::torrent_flags::auto_managed);
 std::wstring state=s.errc?tr(L"Ошибка",L"Error"):item.stopped?tr(L"Остановлено",L"Stopped"):item.selectPending?tr(L"Выбор файлов",L"Choose files"):paused?tr(L"Пауза",L"Paused"):!item.handle.torrent_file()?tr(L"Получение списка файлов",L"Fetching metadata"):s.is_seeding?tr(L"Завершено · Раздача",L"Complete · Seeding"):s.state==lt::torrent_status::checking_files?tr(L"Проверка",L"Checking"):(s.flags&lt::torrent_flags::paused)?tr(L"В очереди",L"Queued"):tr(L"Загрузка",L"Downloading");
 c.box(D2D1::RectF(18,y,width-18,y+124),paused?(c.dark?0x233955:0xe5f0ff):c.card(),11,c.lineColor());c.label(wide(s.name),D2D1::RectF(34,y+12,width-110,y+42),19,true);c.label(sizeText(item.selectPending&&item.handle.torrent_file()?item.handle.torrent_file()->total_size():s.total_wanted)+L" · "+state,D2D1::RectF(34,y+43,width-110,y+68),15,false,c.muted());
 if(!item.selectPending&&s.is_seeding)c.success(width-51,y+42);else if(!item.selectPending)c.label(std::to_wstring(int(s.progress*100))+L"%",D2D1::RectF(width-93,y+30,width-34,y+58),18,true,0,DWRITE_TEXT_ALIGNMENT_TRAILING);
 c.box(D2D1::RectF(34,y+77,width-34,y+85),c.dark?0x394a63:0xe8e9ec,4);if(!item.selectPending&&s.progress>0)c.box(D2D1::RectF(34,y+77,34+(width-68)*s.progress,y+85),0x328cff,4);
 std::wstring eta=item.selectPending?L"":paused?tr(L"Пауза",L"Paused"):s.is_finished?L"100%":L"";if(!item.selectPending&&s.download_payload_rate>0&&!s.is_finished&&!paused){auto seconds=(s.total_wanted-s.total_wanted_done)/s.download_payload_rate;eta=tr(L"Осталось ",L"Remaining ")+std::to_wstring(seconds/60)+tr(L" мин ",L" min ")+std::to_wstring(seconds%60)+tr(L" сек",L" sec");}c.label(eta,D2D1::RectF(width-310,y+92,width-34,y+116),14,false,c.muted(),DWRITE_TEXT_ALIGNMENT_TRAILING);}
 c.rt->PopAxisAlignedClip();c.end();}EndPaint(w,&ps);}
static void context(int row,POINT p) {std::vector<visual::MenuItem> entries;if(row<0){entries={{1,tr(L"Открыть .torrent",L"Open .torrent")},{2,tr(L"Добавить magnet",L"Add magnet")}};}else {entries={{3,tr(L"Пауза",L"Pause")},{4,tr(L"Возобновить",L"Resume")},{5,tr(L"Остановить",L"Stop")},{6,tr(L"Удалить из списка",L"Remove from list")},{0,L""},{7,tr(L"Приоритет выше",L"Higher priority")},{8,tr(L"Приоритет ниже",L"Lower priority")}};}int id=visual::menu(mainWindow,p,std::move(entries),prefs.dark!=0);
 if(id==1) openTorrent(); if(id==2) {Dialog d{0};if(dialog(d)&&!d.result.empty()) add(d.result);} if(row>=0&&id>=3) {auto& item=engine->items[row];auto h=item.handle; if(id==3||id==5) {h.unset_flags(lt::torrent_flags::auto_managed);h.pause(); item.stopped=id==5;} if(id==4) {item.stopped=false; if(!item.selectPending) h.set_flags(lt::torrent_flags::auto_managed);h.resume();} if(id==6) engine->remove(h); if(id==7) {h.queue_position_up();if(row>0) std::swap(engine->items[row],engine->items[row-1]);} if(id==8) {h.queue_position_down();if(row+1<int(engine->items.size())) std::swap(engine->items[row],engine->items[row+1]);} engine->save();} InvalidateRect(mainWindow,nullptr,FALSE);
}
static LRESULT CALLBACK windowProc(HWND w,UINT m,WPARAM wp,LPARAM lp) {
 try {
 switch(m) {
 case WM_CREATE: visual::frame(w,prefs.dark!=0);{UINT dpi=GetDpiForWindow(w);HICON small=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXSMICON,dpi),GetSystemMetricsForDpi(SM_CYSMICON,dpi),LR_DEFAULTCOLOR));SendMessageW(w,WM_SETICON,ICON_SMALL,LPARAM(small));}mainWindow=w;SetTimer(w,1,1000,nullptr);DragAcceptFiles(w,TRUE);return 0;
 case WM_PAINT:paint(w);return 0;
 case WM_APP+1:if(interactionTesting){Dialog d{1};dialog(d);}return 0;
 case WM_APP+2:if(interactionTesting)add((std::filesystem::current_path()/L"pending.torrent").wstring());return 0;
 case WM_ERASEBKGND:return 1;
 case WM_GETMINMAXINFO: {auto info=reinterpret_cast<MINMAXINFO*>(lp);info->ptMinTrackSize={460,320};return 0;}
 case WM_SIZE:InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_DPICHANGED: {auto r=reinterpret_cast<RECT*>(lp);SetWindowPos(w,nullptr,r->left,r->top,r->right-r->left,r->bottom-r->top,SWP_NOZORDER);mainCanvas.rt.Reset();mainCanvas.ink.Reset();HICON small=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXSMICON,HIWORD(wp)),GetSystemMetricsForDpi(SM_CYSMICON,HIWORD(wp)),LR_DEFAULTCOLOR));HICON previous=HICON(SendMessageW(w,WM_SETICON,ICON_SMALL,LPARAM(small)));if(previous)DestroyIcon(previous);return 0;}
 case WM_MOUSEWHEEL: {RECT r;GetClientRect(w,&r);int viewport=int(r.bottom/(GetDpiForWindow(w)/96.f))-86; scroll=std::clamp(scroll-GET_WHEEL_DELTA_WPARAM(wp)/3,0,std::max(0,int(engine->items.size())*136-viewport));InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_LBUTTONUP: {float scale=GetDpiForWindow(w)/96.f;int x=int(GET_X_LPARAM(lp)/scale),y=int(GET_Y_LPARAM(lp)/scale);RECT r;GetClientRect(w,&r);int width=int(r.right/scale); if(y>=22&&y<=60&&x>=width-60) {Dialog d{1};dialog(d);} else if(y>=22&&y<=60&&x>=width-112) {POINT p;GetCursorPos(&p);context(-1,p);} else {int row=rowAt(y);if(row>=0) {Dialog d{2};d.handle=engine->items[row].handle;dialog(d);}}return 0;}
 case WM_CONTEXTMENU: {POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)},local=p;ScreenToClient(w,&local);context(rowAt(int(local.y/(GetDpiForWindow(w)/96.f))),p);return 0;}
 case WM_DROPFILES: {HDROP drop=HDROP(wp);UINT count=DragQueryFileW(drop,0xffffffff,nullptr,0);for(UINT i=0;i<count;++i){wchar_t path[32768];DragQueryFileW(drop,i,path,32768);add(path);}DragFinish(drop);return 0;}
 case WM_COPYDATA: {auto c=reinterpret_cast<COPYDATASTRUCT*>(lp);if(c->dwData==1&&c->cbData>=sizeof(wchar_t)&&c->cbData<=65536&&reinterpret_cast<wchar_t*>(c->lpData)[c->cbData/sizeof(wchar_t)-1]==0) add(static_cast<wchar_t*>(c->lpData));SetForegroundWindow(w);return TRUE;}
 case WM_TIMER: {if(wp==99){KillTimer(w,99);visual::capture(w,L"main.png");Dialog settings{1};dialog(settings);Dialog files{2};files.fixture=true;files.names={L"01. Серия 1.mkv",L"02. Серия 2.mkv",L"03. Серия 3.mkv",L"04. Постер.jpg",L"05. Readme.txt"};files.sizes={L"1,2 ГБ",L"1,3 ГБ",L"1,3 ГБ",L"3,4 МБ",L"12 КБ"};files.priorities={0,7,7,1,0};files.remembered={4,7,7,1,4};dialog(files);PostMessageW(w,WM_CLOSE,0,0);return 0;}engine->tick(); static int seconds=0;if(++seconds%30==0)engine->save();if(!engine->errors.empty()){auto e=engine->errors.back();engine->errors.clear();error(e);}InvalidateRect(w,nullptr,FALSE);if(IsWindowEnabled(w)){for(auto& item:engine->items)if(item.selectPending&&!item.selectionShown&&item.handle.torrent_file()){item.selectionShown=true;Dialog d{2};d.handle=item.handle;dialog(d);break;}}return 0;}
 case WM_CLOSE:if(!closing){closing=true;KillTimer(w,1);EnableWindow(w,FALSE);engine->shutdown();DestroyWindow(w);}return 0;
 case WM_DESTROY:PostQuitMessage(0);return 0;
 }
 } catch(std::exception const& e) {if(smokeTesting){std::ofstream("Torrent-startup-error.txt")<<e.what();PostQuitMessage(1);return 0;}if(m==WM_CLOSE){closing=false;EnableWindow(w,TRUE);SetTimer(w,1,1000,nullptr);}error(e.what());}
 return DefWindowProcW(w,m,wp,lp);
}
int WINAPI wWinMain(HINSTANCE h,HINSTANCE,PWSTR,int show) {
 instance=h;SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED); InitCommonControls();
 int argc=0;auto argv=CommandLineToArgvW(GetCommandLineW(),&argc);
 interactionTesting=argc==2&&std::wstring(argv[1])==L"--interaction-test";designPreview=argc==2&&std::wstring(argv[1])==L"--visual-test";bool smoke=interactionTesting||designPreview||(argc==2&&std::wstring(argv[1])==L"--smoke-test");
 smokeTesting=smoke;stage("entered wWinMain");
 HANDLE mutex=CreateMutexW(nullptr,FALSE,smoke?L"Local\\Torrent.Native.Client.Test.v1":L"Local\\Torrent.Native.Client.v1"); if(GetLastError()==ERROR_ALREADY_EXISTS) {HWND existing=FindWindowW(L"TorrentMain",nullptr);if(existing){for(int i=1;i<argc;++i){COPYDATASTRUCT c{1,DWORD((wcslen(argv[i])+1)*sizeof(wchar_t)),argv[i]};SendMessageW(existing,WM_COPYDATA,0,LPARAM(&c));}ShowWindow(existing,SW_RESTORE);SetForegroundWindow(existing);}LocalFree(argv);CloseHandle(mutex);CoUninitialize();return 0;}
 try {stage("resolving folders");dataRoot=smoke?std::filesystem::current_path()/(interactionTesting?L"ui-test-data":L"smoke-data"):knownFolder(FOLDERID_LocalAppData)/L"Torrent";prefs.folder=knownFolder(FOLDERID_Downloads).wstring();std::filesystem::create_directories(dataRoot);std::ifstream f(dataRoot/L"settings.txt",std::ios::binary);std::string folder;if(std::getline(f,folder)){prefs.folder=wide(folder);f>>prefs.action>>prefs.dark>>prefs.english;}prefs.action=std::clamp(prefs.action,0,1);prefs.dark=std::clamp(prefs.dark,0,1);prefs.english=std::clamp(prefs.english,0,1);stage("starting engine");engine=std::make_unique<Engine>(dataRoot/L"session");stage("initializing drawing");
 WNDCLASSW wc{};wc.hInstance=h;wc.lpfnWndProc=windowProc;wc.lpszClassName=L"TorrentMain";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);wc.hIcon=LoadIconW(h,MAKEINTRESOURCEW(1));if(!RegisterClassW(&wc))throw std::runtime_error("Main class registration failed: "+std::to_string(GetLastError()));wc.lpfnWndProc=dialogProc;wc.lpszClassName=L"TorrentDialog";wc.hbrBackground=nullptr;if(!RegisterClassW(&wc))throw std::runtime_error("Dialog class registration failed: "+std::to_string(GetLastError()));
 stage("creating window");HWND w=CreateWindowExW(0,L"TorrentMain",L"Torrent",WS_OVERLAPPEDWINDOW,CW_USEDEFAULT,CW_USEDEFAULT,int(840*GetDpiForSystem()/96.f),int(610*GetDpiForSystem()/96.f),nullptr,nullptr,h,nullptr);if(!w)throw std::runtime_error("Window creation failed: "+std::to_string(GetLastError()));stage("window created");ShowWindow(w,(designPreview||interactionTesting)?SW_SHOWNOACTIVATE:show);if(smoke){paint(w);if(!mainCanvas.rt)throw std::runtime_error("Direct2D target could not be created");stage("Direct2D rendered");if(interactionTesting)PostMessageW(w,WM_APP+1,0,0);else if(designPreview)SetTimer(w,99,250,nullptr);else PostMessageW(w,WM_CLOSE,0,0);}else for(int i=1;i<argc;++i)add(argv[i]);LocalFree(argv);MSG m;while(GetMessageW(&m,nullptr,0,0)>0){TranslateMessage(&m);DispatchMessageW(&m);}engine.reset();if(smoke){stage("clean shutdown");CloseHandle(mutex);CoUninitialize();return int(m.wParam);} } catch(std::exception const& e){if(smoke){std::ofstream("Torrent-startup-error.txt")<<e.what();CloseHandle(mutex);CoUninitialize();return 1;}MessageBoxW(nullptr,wide(e.what()).c_str(),L"Torrent",MB_ICONERROR);}
 CloseHandle(mutex);CoUninitialize();return 0;
}
