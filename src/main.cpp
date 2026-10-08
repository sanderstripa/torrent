#include "engine.h"
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
using Microsoft::WRL::ComPtr;
static HINSTANCE instance; static HWND mainWindow; static std::unique_ptr<Engine> engine;
static std::filesystem::path dataRoot;
struct Preferences { std::wstring folder; int action=0, dark=0, english=0; } prefs;
static ComPtr<ID2D1Factory> factory; static ComPtr<IDWriteFactory> writing; static ComPtr<IDWriteTextFormat> normal,title;
static ComPtr<ID2D1HwndRenderTarget> target; static ComPtr<ID2D1SolidColorBrush> brush;
static int scroll=0; static bool closing=false;
static std::wstring tr(wchar_t const* ru,wchar_t const* en) {return prefs.english?en:ru;}
static void error(std::string const& message) {MessageBoxW(mainWindow,wide(message).c_str(),L"Torrent",MB_OK|MB_ICONERROR);}
static void savePrefs() { std::ofstream f(dataRoot/L"settings.txt",std::ios::binary); f<<utf8(prefs.folder)<<'\n'<<prefs.action<<' '<<prefs.dark<<' '<<prefs.english; }
static std::wstring sizeText(std::int64_t size) { wchar_t b[64]; if(size>=1024LL*1024*1024) swprintf_s(b,L"%.1f GB",double(size)/(1024*1024*1024)); else swprintf_s(b,L"%.1f MB",double(size)/(1024*1024)); return b; }
static HWND control(HWND parent,wchar_t const* cls,std::wstring const& text,DWORD style,int x,int y,int w,int h,int id) { HWND c=CreateWindowExW(0,cls,text.c_str(),WS_CHILD|WS_VISIBLE|style,x,y,w,h,parent,HMENU(INT_PTR(id)),instance,nullptr); SendMessageW(c,WM_SETFONT,WPARAM(GetStockObject(DEFAULT_GUI_FONT)),TRUE); return c; }
static std::wstring textOf(HWND c) {int n=GetWindowTextLengthW(c); std::wstring t(n+1,0); GetWindowTextW(c,t.data(),n+1); t.resize(n); return t;}
static void combo(HWND c,std::initializer_list<std::wstring> values,int selected) {for(auto const& v:values) SendMessageW(c,CB_ADDSTRING,0,LPARAM(v.c_str())); SendMessageW(c,CB_SETCURSEL,selected,0);}
static bool browse(HWND owner,std::wstring& folder) {ComPtr<IFileDialog> d; if(FAILED(CoCreateInstance(CLSID_FileOpenDialog,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&d)))) return false; d->SetOptions(FOS_PICKFOLDERS|FOS_FORCEFILESYSTEM); if(FAILED(d->Show(owner))) return false; ComPtr<IShellItem> item; d->GetResult(&item); PWSTR p=nullptr; if(FAILED(item->GetDisplayName(SIGDN_FILESYSPATH,&p))) return false; folder=p; CoTaskMemFree(p); return true;}
struct Dialog {int kind; bool done=false,accepted=false; lt::torrent_handle handle; std::wstring result; HWND window=nullptr,list=nullptr; std::vector<int> priorities;};
static void fillFiles(Dialog& d) {
 auto ti=d.handle.torrent_file();if(!ti)return; auto ps=d.handle.get_file_priorities();
 for(int i=0;i<ti->num_files();++i) {auto index=lt::file_index_t(i);int p=i<int(ps.size())?static_cast<std::uint8_t>(ps[i]):4;
  bool pending=std::any_of(engine->items.begin(),engine->items.end(),[&](auto const& item){return item.handle==d.handle&&item.selectPending;});if(pending&&p==0)p=4;
  d.priorities.push_back(p);auto name=wide(ti->files().file_path(index));LVITEMW row{};row.mask=LVIF_TEXT;row.iItem=i;row.pszText=name.data();ListView_InsertItem(d.list,&row);
  auto size=sizeText(ti->files().file_size(index));ListView_SetItemText(d.list,i,1,size.data());std::wstring priority=p==0?tr(L"Не скачивать",L"Skip"):p<=1?tr(L"Низкий",L"Low"):p>=7?tr(L"Высокий",L"High"):tr(L"Обычный",L"Normal");ListView_SetItemText(d.list,i,2,priority.data());ListView_SetCheckState(d.list,i,p!=0);
 }
}
static LRESULT CALLBACK dialogProc(HWND w,UINT m,WPARAM wp,LPARAM lp) {
 auto d=reinterpret_cast<Dialog*>(GetWindowLongPtrW(w,GWLP_USERDATA));
 if(m==WM_NCCREATE) {d=static_cast<Dialog*>(reinterpret_cast<CREATESTRUCTW*>(lp)->lpCreateParams); SetWindowLongPtrW(w,GWLP_USERDATA,LONG_PTR(d)); d->window=w;}
 if(!d) return DefWindowProcW(w,m,wp,lp);
 if(m==WM_CREATE) {
  if(d->kind==0) { control(w,L"STATIC",tr(L"Magnet-ссылка",L"Magnet link"),0,20,18,420,24,0); control(w,L"EDIT",L"",WS_BORDER|ES_AUTOHSCROLL,20,48,440,30,10); }
  if(d->kind==1) {
   control(w,L"STATIC",tr(L"Папка загрузок",L"Download folder"),0,20,20,420,24,0);
   control(w,L"EDIT",prefs.folder,WS_BORDER|ES_AUTOHSCROLL,20,48,360,28,10); control(w,L"BUTTON",L"…",0,390,48,60,28,11);
   control(w,L"STATIC",tr(L"При добавлении",L"On adding"),0,20,94,190,24,0); combo(control(w,L"COMBOBOX",L"",CBS_DROPDOWNLIST,220,90,230,180,12),{tr(L"Начать загрузку",L"Start download"),tr(L"Выбрать файлы",L"Choose files")},prefs.action);
   control(w,L"STATIC",tr(L"Тема",L"Theme"),0,20,136,190,24,0); combo(control(w,L"COMBOBOX",L"",CBS_DROPDOWNLIST,220,132,230,150,13),{tr(L"Светлая",L"Light"),tr(L"Тёмная",L"Dark")},prefs.dark);
   control(w,L"STATIC",tr(L"Язык",L"Language"),0,20,178,190,24,0); combo(control(w,L"COMBOBOX",L"",CBS_DROPDOWNLIST,220,174,230,150,14),{L"Русский",L"English"},prefs.english);
  }
  if(d->kind==2) {
   auto ti=d->handle.torrent_file(); control(w,L"STATIC",ti?wide(ti->name()):tr(L"Получение списка файлов…",L"Fetching file list…"),SS_PATHELLIPSIS,20,16,630,26,0);
   d->list=control(w,WC_LISTVIEWW,L"",LVS_REPORT|LVS_SINGLESEL|WS_BORDER,20,50,640,300,20); ListView_SetExtendedListViewStyle(d->list,LVS_EX_CHECKBOXES|LVS_EX_FULLROWSELECT|LVS_EX_DOUBLEBUFFER);
   int widths[]={390,100,145}; std::wstring labels[]={tr(L"Имя файла",L"File"),tr(L"Размер",L"Size"),tr(L"Приоритет",L"Priority")};
   for(int c=0;c<3;++c) {LVCOLUMNW col{}; col.mask=LVCF_TEXT|LVCF_WIDTH; col.pszText=labels[c].data(); col.cx=widths[c]; ListView_InsertColumn(d->list,c,&col);}
   fillFiles(*d);
   control(w,L"STATIC",tr(L"Приоритет выбранного файла",L"Selected file priority"),0,20,370,300,24,0); combo(control(w,L"COMBOBOX",L"",CBS_DROPDOWNLIST,340,366,185,180,21),{tr(L"Низкий",L"Low"),tr(L"Обычный",L"Normal"),tr(L"Высокий",L"High")},1); control(w,L"BUTTON",tr(L"Задать",L"Set"),0,540,366,120,28,22);
  }
  int y=d->kind==2?420:d->kind==1?228:102; int width=d->kind==2?660:450;
  control(w,L"BUTTON",tr(L"Отмена",L"Cancel"),0,width-230,y,110,32,2); control(w,L"BUTTON",tr(L"Применить",L"Apply"),BS_DEFPUSHBUTTON,width-110,y,110,32,1);
  if(d->kind==2&&!d->handle.torrent_file()){EnableWindow(GetDlgItem(w,1),FALSE);SetTimer(w,2,500,nullptr);}return 0;
 }
 if(m==WM_COMMAND) {
  int id=LOWORD(wp);
  if(id==11) {auto folder=textOf(GetDlgItem(w,10)); if(browse(w,folder)) SetWindowTextW(GetDlgItem(w,10),folder.c_str());}
  if(id==22) {int row=ListView_GetNextItem(d->list,-1,LVNI_SELECTED); if(row>=0) {int values[]={1,4,7}; int selection=int(SendDlgItemMessageW(w,21,CB_GETCURSEL,0,0)); d->priorities[row]=values[std::clamp(selection,0,2)]; ListView_SetCheckState(d->list,row,TRUE); std::wstring label=selection==0?tr(L"Низкий",L"Low"):selection==2?tr(L"Высокий",L"High"):tr(L"Обычный",L"Normal"); ListView_SetItemText(d->list,row,2,label.data());}}
  if(id==1) {
   if(d->kind==0) d->result=textOf(GetDlgItem(w,10));
   if(d->kind==1) {auto folder=textOf(GetDlgItem(w,10)); if(folder.empty()||!std::filesystem::path(folder).is_absolute()) {MessageBoxW(w,tr(L"Укажите полный путь к папке",L"Choose an absolute folder path").c_str(),L"Torrent",MB_OK); return 0;} prefs.folder=folder; prefs.action=int(SendDlgItemMessageW(w,12,CB_GETCURSEL,0,0)); prefs.dark=int(SendDlgItemMessageW(w,13,CB_GETCURSEL,0,0)); prefs.english=int(SendDlgItemMessageW(w,14,CB_GETCURSEL,0,0)); savePrefs();}
   if(d->kind==2) {std::vector<lt::download_priority_t> p; for(int i=0;i<int(d->priorities.size());++i) p.push_back(lt::download_priority_t(static_cast<std::uint8_t>(ListView_GetCheckState(d->list,i)?(d->priorities[i]?d->priorities[i]:4):0))); d->handle.prioritize_files(p); for(auto& item:engine->items) if(item.handle==d->handle&&item.selectPending) {item.selectPending=false; item.handle.unset_flags(lt::torrent_flags::default_dont_download);item.handle.set_flags(lt::torrent_flags::auto_managed); item.handle.resume();} engine->save();}
   d->accepted=true; DestroyWindow(w);
  } else if(id==2) DestroyWindow(w); return 0;
 }
 if(m==WM_TIMER&&d->kind==2&&d->handle.torrent_file()){fillFiles(*d);EnableWindow(GetDlgItem(w,1),TRUE);KillTimer(w,2);return 0;}
 if(m==WM_CLOSE) {DestroyWindow(w);return 0;} if(m==WM_DESTROY) {d->done=true; return 0;} return DefWindowProcW(w,m,wp,lp);
}
static bool dialog(Dialog& d) {EnableWindow(mainWindow,FALSE); int width=d.kind==2?700:490,height=d.kind==2?510:d.kind==1?310:185; RECT r; GetWindowRect(mainWindow,&r); HWND w=CreateWindowExW(WS_EX_DLGMODALFRAME,L"TorrentDialog",d.kind==2?tr(L"Файлы торрента",L"Torrent files").c_str():d.kind==1?tr(L"Настройки",L"Settings").c_str():L"Magnet",WS_CAPTION|WS_SYSMENU,r.left+40,r.top+50,width,height,mainWindow,nullptr,instance,&d); ShowWindow(w,SW_SHOW); MSG m; while(!d.done&&GetMessageW(&m,nullptr,0,0)>0) {if(!IsDialogMessageW(w,&m)) {TranslateMessage(&m); DispatchMessageW(&m);}} EnableWindow(mainWindow,TRUE); SetForegroundWindow(mainWindow); InvalidateRect(mainWindow,nullptr,FALSE); return d.accepted;}
static void add(std::wstring const& source) {try {auto h=engine->add(utf8(source),utf8(prefs.folder),prefs.action==1); if(prefs.action==1&&h.torrent_file()) {Dialog d{2};d.handle=h;dialog(d);} InvalidateRect(mainWindow,nullptr,FALSE);} catch(std::exception const& e) {error(e.what());}}
static void openTorrent() {wchar_t file[32768]{}; OPENFILENAMEW o{}; o.lStructSize=sizeof(o); o.hwndOwner=mainWindow; o.lpstrFilter=L"Torrent\0*.torrent\0"; o.lpstrFile=file; o.nMaxFile=32768; o.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR; if(GetOpenFileNameW(&o)) add(file);}
static void color(D2D1_COLOR_F c) {brush->SetColor(c);}
static void text(std::wstring const& s,D2D1_RECT_F r,bool heading=false) {color(prefs.dark?D2D1::ColorF(0xe8edf5):D2D1::ColorF(0x172033)); target->DrawText(s.c_str(),UINT32(s.size()),heading?title.Get():normal.Get(),r,brush.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);}
static void rounded(D2D1_RECT_F r,D2D1_COLOR_F c,float radius=10) {color(c);target->FillRoundedRectangle(D2D1::RoundedRect(r,radius,radius),brush.Get());}
static int rowAt(int y) {int row=(y-90+scroll)/112; return y>=90&&row>=0&&row<int(engine->items.size())?row:-1;}
static void paint(HWND w) {
 PAINTSTRUCT ps; BeginPaint(w,&ps); RECT r;GetClientRect(w,&r); float scale=GetDpiForWindow(w)/96.f; float width=r.right/scale;
 if(!target) {factory->CreateHwndRenderTarget(D2D1::RenderTargetProperties(),D2D1::HwndRenderTargetProperties(w,D2D1::SizeU(r.right,r.bottom)),&target); if(target) {target->SetDpi(96*scale,96*scale);target->CreateSolidColorBrush(D2D1::ColorF(0),&brush);}}
 if(target) {target->BeginDraw(); target->Clear(prefs.dark?D2D1::ColorF(0x151b25):D2D1::ColorF(0xf6f9fd)); text(L"Torrent",D2D1::RectF(24,22,300,62),true);
 rounded(D2D1::RectF(width-112,22,width-70,64),prefs.dark?D2D1::ColorF(0x283445):D2D1::ColorF(0xeaf0f8)); text(L"+",D2D1::RectF(width-101,26,width-70,64),true);
 rounded(D2D1::RectF(width-60,22,width-18,64),prefs.dark?D2D1::ColorF(0x283445):D2D1::ColorF(0xeaf0f8)); text(L"⚙",D2D1::RectF(width-51,27,width-20,64));
 if(engine->items.empty()) text(tr(L"Добавьте торрент кнопкой +",L"Add a torrent with +"),D2D1::RectF(24,130,width-24,180));
 target->PushAxisAlignedClip(D2D1::RectF(0,82,width,r.bottom/scale),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 for(int i=0;i<int(engine->items.size());++i) {auto const& item=engine->items[i]; auto s=item.handle.status(); float y=90.f+i*112-scroll; if(y+104<82||y>r.bottom/scale) continue; rounded(D2D1::RectF(18,y,width-18,y+100),prefs.dark?D2D1::ColorF(0x202a39):D2D1::ColorF(0xffffff)); text(wide(s.name),D2D1::RectF(34,y+12,width-105,y+38));
 std::wstring state=s.errc?tr(L"Ошибка",L"Error"):item.stopped?tr(L"Остановлено",L"Stopped"):item.selectPending?tr(L"Выбор файлов",L"Choose files"):(s.flags&lt::torrent_flags::paused)&&!(s.flags&lt::torrent_flags::auto_managed)?tr(L"Пауза",L"Paused"):!item.handle.torrent_file()?tr(L"Получение списка файлов",L"Fetching metadata"):s.is_seeding?tr(L"Завершено · Раздача",L"Complete · Seeding"):s.state==lt::torrent_status::checking_files?tr(L"Проверка",L"Checking"):(s.flags&lt::torrent_flags::paused)?tr(L"В очереди",L"Queued"):tr(L"Загрузка",L"Downloading");
 text(sizeText(s.total_wanted)+L" · "+state,D2D1::RectF(34,y+38,width-100,y+62)); text(std::to_wstring(int(s.progress*100))+L"%",D2D1::RectF(width-82,y+16,width-30,y+43)); rounded(D2D1::RectF(34,y+68,width-34,y+74),prefs.dark?D2D1::ColorF(0x38465a):D2D1::ColorF(0xe1e9f3),3); if(s.progress>0) rounded(D2D1::RectF(34,y+68,34+(width-68)*s.progress,y+74),D2D1::ColorF(0x328cff),3);
 std::wstring eta=L"—"; if(s.download_payload_rate>0&&!s.is_finished&&!(s.flags&lt::torrent_flags::paused)) {auto seconds=(s.total_wanted-s.total_wanted_done)/s.download_payload_rate; eta=tr(L"Осталось ",L"Remaining ")+std::to_wstring(seconds/60)+tr(L" мин",L" min");} text(eta,D2D1::RectF(width-230,y+78,width-30,y+100)); }
 target->PopAxisAlignedClip(); if(target->EndDraw()==D2DERR_RECREATE_TARGET) {brush.Reset();target.Reset();}}
 EndPaint(w,&ps);
}
static void context(int row,POINT p) {HMENU menu=CreatePopupMenu(); if(row<0) {AppendMenuW(menu,MF_STRING,1,tr(L"Открыть .torrent",L"Open .torrent").c_str()); AppendMenuW(menu,MF_STRING,2,tr(L"Добавить magnet",L"Add magnet").c_str());} else {AppendMenuW(menu,MF_STRING,3,tr(L"Пауза",L"Pause").c_str());AppendMenuW(menu,MF_STRING,4,tr(L"Возобновить",L"Resume").c_str());AppendMenuW(menu,MF_STRING,5,tr(L"Остановить",L"Stop").c_str());AppendMenuW(menu,MF_STRING,6,tr(L"Удалить из списка",L"Remove from list").c_str());AppendMenuW(menu,MF_SEPARATOR,0,nullptr);AppendMenuW(menu,MF_STRING,7,tr(L"Приоритет выше",L"Higher priority").c_str());AppendMenuW(menu,MF_STRING,8,tr(L"Приоритет ниже",L"Lower priority").c_str());}
 int id=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,p.x,p.y,0,mainWindow,nullptr);DestroyMenu(menu);
 if(id==1) openTorrent(); if(id==2) {Dialog d{0};if(dialog(d)&&!d.result.empty()) add(d.result);} if(row>=0&&id>=3) {auto& item=engine->items[row];auto h=item.handle; if(id==3||id==5) {h.unset_flags(lt::torrent_flags::auto_managed);h.pause(); item.stopped=id==5;} if(id==4) {item.stopped=false; if(!item.selectPending) h.set_flags(lt::torrent_flags::auto_managed);h.resume();} if(id==6) engine->remove(h); if(id==7) {h.queue_position_up();if(row>0) std::swap(engine->items[row],engine->items[row-1]);} if(id==8) {h.queue_position_down();if(row+1<int(engine->items.size())) std::swap(engine->items[row],engine->items[row+1]);} engine->save();} InvalidateRect(mainWindow,nullptr,FALSE);
}
static LRESULT CALLBACK windowProc(HWND w,UINT m,WPARAM wp,LPARAM lp) {
 try {
 switch(m) {
 case WM_CREATE: mainWindow=w;SetTimer(w,1,1000,nullptr);DragAcceptFiles(w,TRUE);return 0;
 case WM_PAINT:paint(w);return 0;
 case WM_ERASEBKGND:return 1;
 case WM_SIZE:if(target)target->Resize(D2D1::SizeU(LOWORD(lp),HIWORD(lp)));InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_DPICHANGED: {auto r=reinterpret_cast<RECT*>(lp);SetWindowPos(w,nullptr,r->left,r->top,r->right-r->left,r->bottom-r->top,SWP_NOZORDER);target.Reset();brush.Reset();return 0;}
 case WM_MOUSEWHEEL: {RECT r;GetClientRect(w,&r);int viewport=int(r.bottom/(GetDpiForWindow(w)/96.f))-90; scroll=std::clamp(scroll-GET_WHEEL_DELTA_WPARAM(wp)/3,0,std::max(0,int(engine->items.size())*112-viewport));InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_LBUTTONUP: {float scale=GetDpiForWindow(w)/96.f;int x=int(GET_X_LPARAM(lp)/scale),y=int(GET_Y_LPARAM(lp)/scale);RECT r;GetClientRect(w,&r);int width=int(r.right/scale); if(y>=22&&y<=64&&x>=width-60) {Dialog d{1};dialog(d);} else if(y>=22&&y<=64&&x>=width-112) {POINT p;GetCursorPos(&p);context(-1,p);} else {int row=rowAt(y);if(row>=0) {Dialog d{2};d.handle=engine->items[row].handle;dialog(d);}}return 0;}
 case WM_CONTEXTMENU: {POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)},local=p;ScreenToClient(w,&local);context(rowAt(int(local.y/(GetDpiForWindow(w)/96.f))),p);return 0;}
 case WM_DROPFILES: {HDROP drop=HDROP(wp);UINT count=DragQueryFileW(drop,0xffffffff,nullptr,0);for(UINT i=0;i<count;++i){wchar_t path[32768];DragQueryFileW(drop,i,path,32768);add(path);}DragFinish(drop);return 0;}
 case WM_COPYDATA: {auto c=reinterpret_cast<COPYDATASTRUCT*>(lp);if(c->dwData==1&&c->cbData>=sizeof(wchar_t)&&c->cbData<=65536&&reinterpret_cast<wchar_t*>(c->lpData)[c->cbData/sizeof(wchar_t)-1]==0) add(static_cast<wchar_t*>(c->lpData));SetForegroundWindow(w);return TRUE;}
 case WM_TIMER: {engine->tick(); static int seconds=0;if(++seconds%30==0)engine->save();if(!engine->errors.empty()){auto e=engine->errors.back();engine->errors.clear();error(e);}InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_CLOSE:if(!closing){closing=true;KillTimer(w,1);EnableWindow(w,FALSE);engine->shutdown();DestroyWindow(w);}return 0;
 case WM_DESTROY:PostQuitMessage(0);return 0;
 }
 } catch(std::exception const& e) {if(m==WM_CLOSE){closing=false;EnableWindow(w,TRUE);SetTimer(w,1,1000,nullptr);}error(e.what());}
 return DefWindowProcW(w,m,wp,lp);
}
int WINAPI wWinMain(HINSTANCE h,HINSTANCE,PWSTR,int show) {
 instance=h;SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED); InitCommonControls();
 int argc=0;auto argv=CommandLineToArgvW(GetCommandLineW(),&argc);
 HANDLE mutex=CreateMutexW(nullptr,FALSE,L"Local\\Torrent.Native.Client.v1"); if(GetLastError()==ERROR_ALREADY_EXISTS) {HWND existing=FindWindowW(L"TorrentMain",nullptr);if(existing){for(int i=1;i<argc;++i){COPYDATASTRUCT c{1,DWORD((wcslen(argv[i])+1)*sizeof(wchar_t)),argv[i]};SendMessageW(existing,WM_COPYDATA,0,LPARAM(&c));}ShowWindow(existing,SW_RESTORE);SetForegroundWindow(existing);}LocalFree(argv);CloseHandle(mutex);CoUninitialize();return 0;}
 try {PWSTR local=nullptr,downloads=nullptr;SHGetKnownFolderPath(FOLDERID_LocalAppData,0,nullptr,&local);SHGetKnownFolderPath(FOLDERID_Downloads,0,nullptr,&downloads);dataRoot=std::filesystem::path(local)/L"Torrent";prefs.folder=downloads;CoTaskMemFree(local);CoTaskMemFree(downloads);std::filesystem::create_directories(dataRoot);std::ifstream f(dataRoot/L"settings.txt",std::ios::binary);std::string folder;if(std::getline(f,folder)){prefs.folder=wide(folder);f>>prefs.action>>prefs.dark>>prefs.english;}prefs.action=std::clamp(prefs.action,0,1);prefs.dark=std::clamp(prefs.dark,0,1);prefs.english=std::clamp(prefs.english,0,1);engine=std::make_unique<Engine>(dataRoot/L"session");
 D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,factory.GetAddressOf());DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(writing.GetAddressOf()));writing->CreateTextFormat(L"Segoe UI",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,14,L"",&normal);writing->CreateTextFormat(L"Segoe UI",nullptr,DWRITE_FONT_WEIGHT_SEMI_BOLD,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,24,L"",&title);
 WNDCLASSW wc{};wc.hInstance=h;wc.lpfnWndProc=windowProc;wc.lpszClassName=L"TorrentMain";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);wc.hIcon=LoadIconW(h,MAKEINTRESOURCEW(1));RegisterClassW(&wc);wc.lpfnWndProc=dialogProc;wc.lpszClassName=L"TorrentDialog";wc.hbrBackground=HBRUSH(COLOR_WINDOW+1);RegisterClassW(&wc);
 HWND w=CreateWindowExW(0,L"TorrentMain",L"Torrent",WS_OVERLAPPEDWINDOW,CW_USEDEFAULT,CW_USEDEFAULT,840,610,nullptr,nullptr,h,nullptr);ShowWindow(w,show);for(int i=1;i<argc;++i)add(argv[i]);LocalFree(argv);MSG m;while(GetMessageW(&m,nullptr,0,0)>0){TranslateMessage(&m);DispatchMessageW(&m);}engine.reset(); } catch(std::exception const& e){MessageBoxW(nullptr,wide(e.what()).c_str(),L"Torrent",MB_ICONERROR);}
 CloseHandle(mutex);CoUninitialize();return 0;
}
