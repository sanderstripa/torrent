#include "engine.h"
#include "visual.h"
#include "settings_backdrop.h"
#include "window_position.h"
#include "popup.h"
#include "file_list_sort.h"
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
#include <numeric>
using Microsoft::WRL::ComPtr;
static HINSTANCE instance; static HWND mainWindow; static std::unique_ptr<Engine> engine;
static std::filesystem::path dataRoot;
static std::filesystem::path knownFolder(REFKNOWNFOLDERID id) {PWSTR value=nullptr; if(FAILED(SHGetKnownFolderPath(id,KF_FLAG_DONT_VERIFY,nullptr,&value)))throw std::runtime_error("Cannot resolve Windows user folder");std::filesystem::path result(value);CoTaskMemFree(value);return result;}
struct Preferences { std::wstring folder; int action=0, dark=0, english=0; } prefs;
static bool windowsDark=false;static int themeTestOverride=-1;
static bool darkMode(){return prefs.dark==2?windowsDark:prefs.dark==1;}
static void readWindowsTheme(){DWORD light=1,size=sizeof light;RegGetValueW(HKEY_CURRENT_USER,L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",L"AppsUseLightTheme",RRF_RT_REG_DWORD,nullptr,&light,&size);windowsDark=themeTestOverride>=0?themeTestOverride!=0:light==0;}
static void refreshTheme();
static int mainHover=-1,mainFocused=-1;static bool mainKeyboardFocus=false;
static int mainHit(float x,float y,float width){if(y<13||y>=51)return -1;if(x>=width-46&&x<width-14)return 3;if(x>=width-86&&x<width-54)return 2;if(x>=width-126&&x<width-94)return 0;return -1;}
static int scroll=0; static bool closing=false;
static bool smokeTesting=false;static bool designPreview=false,interactionTesting=false;
static void stage(char const* value) {if(smokeTesting)std::ofstream("Torrent-startup-stages.txt",std::ios::app)<<value<<'\n';}
static std::wstring tr(wchar_t const* ru,wchar_t const* en) {return prefs.english?en:ru;}
static void error(std::string const& message);
static bool savePrefs() {auto temporary=dataRoot/L"settings.tmp";{std::ofstream f(temporary,std::ios::binary|std::ios::trunc);f<<utf8(prefs.folder)<<'\n'<<prefs.action<<' '<<prefs.dark<<' '<<prefs.english;f.flush();if(!f)return false;}return MoveFileExW(temporary.c_str(),(dataRoot/L"settings.txt").c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)!=0; }
static std::wstring sizeText(std::int64_t size) { wchar_t b[64];double divisor=1024;std::wstring unit=tr(L"КБ",L"KB");if(size>=1024LL*1024*1024){divisor=1024LL*1024*1024;unit=tr(L"ГБ",L"GB");}else if(size>=1024*1024){divisor=1024*1024;unit=tr(L"МБ",L"MB");}swprintf_s(b,L"%.1f",double(size)/divisor);std::wstring value=b;if(!prefs.english)std::replace(value.begin(),value.end(),L'.',L',');return value+L" "+unit; }
static HWND control(HWND parent,wchar_t const* cls,std::wstring const& text,DWORD style,int x,int y,int w,int h,int id) { HWND c=CreateWindowExW(0,cls,text.c_str(),WS_CHILD|WS_VISIBLE|style,x,y,w,h,parent,HMENU(INT_PTR(id)),instance,nullptr); SendMessageW(c,WM_SETFONT,WPARAM(GetStockObject(DEFAULT_GUI_FONT)),TRUE); return c; }
static std::wstring textOf(HWND c) {int n=GetWindowTextLengthW(c); std::wstring t(n+1,0); GetWindowTextW(c,t.data(),n+1); t.resize(n); return t;}
static bool browse(HWND owner,std::wstring& folder) {ComPtr<IFileDialog> d; if(FAILED(CoCreateInstance(CLSID_FileOpenDialog,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&d)))) return false; d->SetOptions(FOS_PICKFOLDERS|FOS_FORCEFILESYSTEM); if(FAILED(d->Show(owner))) return false; ComPtr<IShellItem> item; d->GetResult(&item); PWSTR p=nullptr; if(FAILED(item->GetDisplayName(SIGDN_FILESYSPATH,&p))) return false; folder=p; CoTaskMemFree(p); return true;}

struct Dialog {
 int kind; bool done=false,accepted=false; lt::torrent_handle handle; std::wstring result;
 HWND window=nullptr;std::vector<int> priorities,remembered;std::vector<std::wstring> names,sizes;std::vector<std::int64_t> bytes;std::vector<int> displayOrder;
 int sortBy=0;bool ascending=true;float nameWidth=0,horizontal=0,horizontalMax=0;bool draggingNameScroll=false;float dragStartX=0,dragStartOffset=0;
 visual::Canvas canvas;int offset=0,hover=-1,focused=1;Preferences draft;HFONT font=nullptr;HBRUSH background=nullptr;bool fixture=false,folderValid=true,saveFailed=false,keyboardFocus=false;
};
static void fillFiles(Dialog& d) {
 auto ti=d.handle.torrent_file();if(!ti)return;auto ps=d.handle.get_file_priorities();
 d.priorities.clear();d.remembered.clear();d.names.clear();d.sizes.clear();d.bytes.clear();d.displayOrder.clear();
 d.nameWidth=0;d.horizontal=0;d.horizontalMax=0;d.offset=0;
 bool pending=std::any_of(engine->items.begin(),engine->items.end(),[&](auto const& item){return item.handle==d.handle&&item.selectPending;});
 for(int i=0;i<ti->num_files();++i){auto index=lt::file_index_t(i);int p=i<int(ps.size())?static_cast<std::uint8_t>(ps[i]):4;if(pending&&p==0)p=4;
 d.priorities.push_back(p);d.remembered.push_back(p?p:4);d.names.push_back(std::filesystem::path(wide(ti->files().file_path(index))).filename().wstring());d.sizes.push_back(sizeText(ti->files().file_size(index)));d.bytes.push_back(ti->files().file_size(index));d.displayOrder.push_back(i);}
 if(d.sortBy)d.displayOrder=filelist::sortedIndices(d.names,d.bytes,d.sortBy,d.ascending);
}
static int fileIndex(Dialog const& d,int row){return row>=0&&row<int(d.displayOrder.size())?d.displayOrder[row]:row;}
static int maxFilesScroll(Dialog const& d){return std::max(0,int(d.priorities.size())*42-std::max(0,int(d.canvas.height-(d.horizontalMax>0?239.f:214.f))));}
static D2D1_RECT_F fileScrollTrack(Dialog const& d){return D2D1::RectF(98,d.canvas.height-88,d.canvas.width-254,d.canvas.height-78);}
static D2D1_RECT_F fileScrollThumb(Dialog const& d){
 auto r=fileScrollTrack(d);float length=r.right-r.left;
 float thumb=std::clamp(length*length/(length+d.horizontalMax),28.f,length);
 float left=r.left+(length-thumb)*(d.horizontalMax>0?d.horizontal/d.horizontalMax:0.f);
 return D2D1::RectF(left,r.top,left+thumb,r.bottom);
}
static void sortDialogFiles(Dialog& d,int by){
 int selected=d.focused>=100&&d.focused<100+int(d.priorities.size())?fileIndex(d,d.focused-100):-1;
 if(d.sortBy==by)d.ascending=!d.ascending;else{d.sortBy=by;d.ascending=true;}
 d.displayOrder=filelist::sortedIndices(d.names,d.bytes,d.sortBy,d.ascending);
 if(selected>=0){auto it=std::find(d.displayOrder.begin(),d.displayOrder.end(),selected);if(it!=d.displayOrder.end())d.focused=100+int(it-d.displayOrder.begin());}
 d.offset=std::clamp(d.offset,0,maxFilesScroll(d));
}
static std::wstring priorityLabel(int p){return p==0?tr(L"Не скачивать",L"Skip"):p<=1?tr(L"Низкий",L"Low"):p>=7?tr(L"Высокий",L"High"):tr(L"Обычный",L"Normal");}
static D2D1_RECT_F settingRect(int id,float width){if(id>=13&&id<=15){float size=(width-64)/3.f;float x=24+(id-13)*(size+8);return D2D1::RectF(x,251,x+size,283);}if(id==16)return D2D1::RectF(width-96,211,width-64,239);if(id==17)return D2D1::RectF(width-56,211,width-24,239);return D2D1::RectF(width-250,165,width-24,201);}
static void drawDialog(HWND w,Dialog& d){PAINTSTRUCT ps;BeginPaint(w,&ps);auto& c=d.canvas;c.dark=darkMode();
 if(c.begin(w)){float width=c.width,height=c.height;
 c.titlebar(d.kind==2?tr(L"Файлы торрента",L"Torrent files"):d.kind==1?tr(L"Настройки",L"Settings"):d.kind==3?tr(L"Ошибка",L"Error"):tr(L"Magnet-ссылка",L"Magnet link"));
 if(d.kind==3){c.label(L"!",D2D1::RectF(24,65,45,90),18,true,0xc95050);c.paragraph(d.result,D2D1::RectF(56,67,width-24,height-70));}else if(d.kind==2){auto ti=d.fixture?std::shared_ptr<lt::torrent_info const>{}:d.handle.torrent_file();c.label(d.fixture?L"The Witcher S01":ti?wide(ti->name()):tr(L"Получение списка файлов…",L"Fetching file list…"),D2D1::RectF(24,50,width-24,76),17,true);
 c.label(d.fixture?tr(L"12,6 ГБ · 10 файлов",L"12.6 GB · 10 files"):ti?sizeText(ti->total_size())+L" · "+std::to_wstring(ti->num_files())+tr(L" файлов",L" files"):L"",D2D1::RectF(24,76,width-24,98),13,false,c.muted());
 c.box(D2D1::RectF(18,112,width-18,height-70),c.card(),9,c.lineColor());c.box(D2D1::RectF(18,112,width-18,143),c.dark?0x191919:0xf0f0ee,7);
 if(d.displayOrder.size()!=d.priorities.size()){d.displayOrder.resize(d.priorities.size());std::iota(d.displayOrder.begin(),d.displayOrder.end(),0);}
 if(d.nameWidth<=0)for(auto const& name:d.names)d.nameWidth=std::max(d.nameWidth,c.textWidth(name,visual::ui::body));
 float nameViewport=std::max(1.f,width-352.f);
 d.horizontalMax=std::max(0.f,d.nameWidth+16.f-nameViewport);d.horizontal=std::clamp(d.horizontal,0.f,d.horizontalMax);d.offset=std::clamp(d.offset,0,maxFilesScroll(d));
 float listBottom=d.horizontalMax>0?height-96:height-71;
 c.check(32,120,!d.priorities.empty()&&std::all_of(d.priorities.begin(),d.priorities.end(),[](int p){return p>0;}));
 c.label(tr(L"Имя файла",L"File name"),D2D1::RectF(68,112,width-281,143),13,false,d.sortBy==1?c.fg():c.muted());
 if(d.sortBy==1)c.label(d.ascending?L"↑":L"↓",D2D1::RectF(width-281,112,width-258,143),14,false,c.fg());
 c.label(tr(L"Размер",L"Size"),D2D1::RectF(width-242,112,width-178,143),13,false,d.sortBy==2?c.fg():c.muted());
 if(d.sortBy==2)c.label(d.ascending?L"↑":L"↓",D2D1::RectF(width-178,112,width-156,143),14,false,c.fg());
 c.label(tr(L"Приоритет",L"Priority"),D2D1::RectF(width-148,112,width-30,143),13,false,c.muted());
 c.rt->PushAxisAlignedClip(D2D1::RectF(19,143,width-19,listBottom),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 for(int row=0;row<int(d.priorities.size());++row){float y=143.f+row*42-d.offset;if(y+42<143||y>listBottom)continue;
 int i=fileIndex(d,row);
 if(d.focused==100+row)c.box(D2D1::RectF(20,y+1,width-20,y+41),c.selected(),0);c.check(32,y+13,d.priorities[i]!=0);auto extension=std::filesystem::path(d.names[i]).extension().wstring();std::transform(extension.begin(),extension.end(),extension.begin(),[](wchar_t ch){return wchar_t(towlower(ch));});c.box(D2D1::RectF(70,y+12,84,y+30),c.muted(),2);if(extension==L".mkv"||extension==L".mp4"||extension==L".avi"){c.line(75,y+17,80,y+21,c.card(),1.7f);c.line(80,y+21,75,y+25,c.card(),1.7f);c.line(75,y+25,75,y+17,c.card(),1.7f);}else if(extension==L".jpg"||extension==L".png"||extension==L".jpeg"){c.line(72,y+27,76,y+21,c.card());c.line(76,y+21,79,y+25,c.card());c.line(79,y+25,81,y+22,c.card());}else {c.line(73,y+18,81,y+18,c.card(),1);c.line(73,y+22,81,y+22,c.card(),1);c.line(73,y+26,78,y+26,c.card(),1);}
 c.rt->PushAxisAlignedClip(D2D1::RectF(98,y,width-254,y+42),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 c.label(d.names[i],D2D1::RectF(98-d.horizontal,y,98-d.horizontal+std::max(d.nameWidth+12.f,nameViewport+12.f),y+42),visual::ui::body);
 c.rt->PopAxisAlignedClip();
 c.label(d.sizes[i],D2D1::RectF(width-242,y,width-158,y+42),13,false,c.muted());
 int p=d.priorities[i];c.box(D2D1::RectF(width-150,y+3,width-30,y+39),c.card(),visual::ui::radius,c.lineColor());c.label(priorityLabel(p),D2D1::RectF(width-140,y+6,width-48,y+36),visual::ui::body,false,p>=7?c.fg():c.muted());c.chevron(width-47,y+18);c.line(20,y+42,width-20,y+42,c.lineColor(),1);}
 c.rt->PopAxisAlignedClip();
 if(d.horizontalMax>0){c.box(fileScrollTrack(d),c.track(),5);c.box(fileScrollThumb(d),c.dark?0xa7aaad:0x797d82,5);}
 }else if(d.kind==1){
 c.label(tr(L"Папка загрузок",L"Download folder"),D2D1::RectF(24,67,width-24,95),visual::ui::help,false,c.muted());c.box(D2D1::RectF(24,103,width-78,139),c.card(),visual::ui::radius,c.lineColor());c.button(L"…",D2D1::RectF(width-66,103,width-24,139),false,d.hover==11);
 c.label(tr(L"При добавлении",L"On adding"),D2D1::RectF(24,163,width-250,203),visual::ui::body);c.box(settingRect(12,width),c.card(),visual::ui::radius,c.lineColor());c.label(d.draft.action?tr(L"Выбрать файлы",L"Choose files"):tr(L"Начать загрузку",L"Start download"),D2D1::RectF(width-236,163,width-48,203),visual::ui::body);c.chevron(width-44,181);
 c.label(tr(L"ВНЕШНИЙ ВИД",L"APPEARANCE"),D2D1::RectF(24,211,width-108,239),12,false,0,DWRITE_TEXT_ALIGNMENT_LEADING,1.f);
 for(int id=13;id<=17;++id){auto r=settingRect(id,width);bool selected=id<=15?d.draft.dark==(id==13?2:id==14?0:1):d.draft.english==id-16;c.box(r,selected?c.selected():c.background(),8,selected?c.muted():c.lineColor());if(d.keyboardFocus&&GetFocus()==w&&d.focused==id)c.focus(r);std::wstring label=id==13?tr(L"Система",L"System"):id==14?tr(L"Светлая",L"Light"):id==15?tr(L"Тёмная",L"Dark"):id==16?L"RU":L"EN";
 if(id<=15){float textWidth=c.textWidth(label,12),left=(r.left+r.right-13-5-textWidth)/2;c.themeIcon(id-13,left,260.5f,selected?c.fg():c.muted());c.label(label,D2D1::RectF(left+18,r.top,left+18+textWidth+1,r.bottom),12,false,selected?c.fg():c.muted());}else c.label(label,r,11,false,selected?c.fg():c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 if(d.keyboardFocus&&GetFocus()==w&&(d.focused==11||d.focused==12))c.focus(d.focused==11?D2D1::RectF(width-66,103,width-24,139):settingRect(12,width));
 if(d.saveFailed)c.label(tr(L"Не удалось сохранить настройки",L"Could not save settings"),D2D1::RectF(24,312,width-24,333),12,false,0xc95050);
 if(!d.folderValid)c.label(tr(L"Укажите полный путь к папке",L"Enter an absolute folder path"),D2D1::RectF(24,141,width-24,160),12,false,0xc95050);
 }else {c.label(tr(L"Вставьте ссылку",L"Paste a magnet link"),D2D1::RectF(24,62,width-24,94),visual::ui::help,false,c.muted());c.box(D2D1::RectF(24,102,width-24,138),c.card(),visual::ui::radius,c.lineColor());}
 if((d.kind==0||d.kind==1)&&GetFocus()==GetDlgItem(w,10))c.focus(d.kind==1?D2D1::RectF(24,103,width-78,139):D2D1::RectF(24,102,width-24,138));
 if(d.kind!=1){if(GetFocus()==w&&(d.focused==1||d.focused==2)){float x=d.focused==1?width-131:width-243;c.focus(D2D1::RectF(x,height-57,x+110,height-15));}
 if(d.kind!=3)c.button(tr(L"Отмена",L"Cancel"),D2D1::RectF(width-240,height-54,width-136,height-18),false,d.hover==2);c.button(d.kind==3?L"OK":tr(L"Применить",L"Apply"),D2D1::RectF(width-128,height-54,width-24,height-18),true,d.hover==1);}
 c.end();}EndPaint(w,&ps);}
static int dialogHit(Dialog& d,float x,float y){auto& c=d.canvas;if(d.kind!=1&&visual::hit(x,y,D2D1::RectF(c.width-240,c.height-54,c.width-136,c.height-18)))return 2;if(d.kind!=1&&visual::hit(x,y,D2D1::RectF(c.width-128,c.height-54,c.width-24,c.height-18)))return 1;if(x>c.width-46&&y<50)return 2;if(d.kind==1){if(y>=101&&y<=141&&x>c.width-66)return 11;for(int id=12;id<=17;++id)if(visual::hit(x,y,settingRect(id,c.width)))return id;}if(d.kind==2&&y>=112&&y<143){if(x>=68&&x<c.width-254)return 20;if(x>=c.width-242&&x<c.width-156)return 21;}if(d.kind==2&&d.horizontalMax>0&&y>=c.height-94&&y<c.height-71&&x>=98&&x<=c.width-254)return 23;return -1;}
static void liveSettings(HWND w,Dialog& d){
 auto folder=textOf(GetDlgItem(w,10));d.folderValid=!folder.empty()&&folder.find_first_of(L"\r\n")==std::wstring::npos&&std::filesystem::path(folder).is_absolute();
 if(d.folderValid)d.draft.folder=folder;else d.draft.folder=prefs.folder;
 bool themeChanged=prefs.dark!=d.draft.dark,languageChanged=prefs.english!=d.draft.english;prefs=d.draft;d.saveFailed=!savePrefs();if(themeChanged){DeleteObject(d.background);d.background=CreateSolidBrush(darkMode()?RGB(16,16,16):RGB(250,250,250));visual::frame(mainWindow,darkMode(),true);visual::frame(w,darkMode(),true);InvalidateRect(GetDlgItem(w,10),nullptr,TRUE);}if(languageChanged)SetWindowTextW(w,tr(L"Настройки",L"Settings").c_str());InvalidateRect(w,nullptr,FALSE);if(themeChanged||languageChanged)InvalidateRect(mainWindow,nullptr,FALSE);
}
static LRESULT CALLBACK editProc(HWND w,UINT m,WPARAM wp,LPARAM lp,UINT_PTR,DWORD_PTR){if(m==WM_SETTEXT){auto result=DefSubclassProc(w,m,wp,lp);SendMessageW(GetParent(w),WM_COMMAND,MAKEWPARAM(GetDlgCtrlID(w),EN_CHANGE),LPARAM(w));return result;}if(m==WM_CONTEXTMENU){HWND owner=GetParent(w);DWORD start=0,end=0;SendMessageW(w,EM_GETSEL,WPARAM(&start),LPARAM(&end));POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};if(point.x==-1&&point.y==-1){RECT r;GetWindowRect(w,&r);point={r.left+12,r.bottom};}int id=visual::menu(owner,point,{{5,tr(L"Отменить ввод",L"Undo"),false,SendMessageW(w,EM_CANUNDO,0,0)!=0},{0,L""},{1,tr(L"Вырезать",L"Cut"),false,start!=end},{2,tr(L"Копировать",L"Copy"),false,start!=end},{3,tr(L"Вставить",L"Paste"),false,IsClipboardFormatAvailable(CF_UNICODETEXT)!=0},{6,tr(L"Удалить",L"Delete"),false,start!=end},{4,tr(L"Выделить всё",L"Select all")}},darkMode());if(id==1)SendMessageW(w,WM_CUT,0,0);if(id==2)SendMessageW(w,WM_COPY,0,0);if(id==3)SendMessageW(w,WM_PASTE,0,0);if(id==4)SendMessageW(w,EM_SETSEL,0,-1);if(id==5)SendMessageW(w,WM_UNDO,0,0);if(id==6)SendMessageW(w,EM_REPLACESEL,TRUE,LPARAM(L""));SetFocus(w);return 0;}return DefSubclassProc(w,m,wp,lp);}
static void acceptDialog(HWND w,Dialog& d){if(d.kind==1){liveSettings(w,d);return;}

 if(d.kind==0)d.result=textOf(GetDlgItem(w,10));

 if(d.kind==2){if(!d.handle.torrent_file())return;std::vector<lt::download_priority_t> p;for(int value:d.priorities)p.push_back(lt::download_priority_t(static_cast<std::uint8_t>(value)));d.handle.prioritize_files(p);for(auto& item:engine->items)if(item.handle==d.handle&&item.selectPending){item.selectPending=false;item.handle.unset_flags(lt::torrent_flags::default_dont_download);item.handle.set_flags(lt::torrent_flags::auto_managed);item.handle.resume();}engine->save();}
 d.accepted=true;DestroyWindow(w);
}
static LRESULT CALLBACK dialogProc(HWND w,UINT m,WPARAM wp,LPARAM lp){auto d=reinterpret_cast<Dialog*>(GetWindowLongPtrW(w,GWLP_USERDATA));if(m==WM_NCCREATE){d=static_cast<Dialog*>(reinterpret_cast<CREATESTRUCTW*>(lp)->lpCreateParams);SetWindowLongPtrW(w,GWLP_USERDATA,LONG_PTR(d));d->window=w;}if(!d)return DefWindowProcW(w,m,wp,lp);if(d->canvas.animationMessage(w,m,wp))return 0;
 switch(m){case WM_CREATE:{d->draft=prefs;visual::frame(w,darkMode(),true);float scale=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);int width=int(r.right/scale);d->canvas.begin(w);d->canvas.end();d->font=CreateFontW(-int(visual::ui::body*scale),0,0,0,FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,d->canvas.fontFamily());d->background=CreateSolidBrush(darkMode()?RGB(16,16,16):RGB(250,250,250));
 if(d->kind==0||d->kind==1){HWND edit=control(w,L"EDIT",d->kind==1?prefs.folder:L"",ES_AUTOHSCROLL|WS_TABSTOP,int(34*scale),int(111*scale),int((width-(d->kind==1?122:68))*scale),int(22*scale),10);SendMessageW(edit,WM_SETFONT,WPARAM(d->font),TRUE);SetWindowSubclass(edit,editProc,1,0);SetFocus(edit);d->focused=10;}else if(d->kind==2&&!d->fixture){fillFiles(*d);if(!d->handle.torrent_file())SetTimer(w,2,500,nullptr);}if(designPreview)SetTimer(w,99,250,nullptr);return 0;}
 case WM_SYSCOMMAND:if((wp&0xfff0)==SC_MAXIMIZE)return 0;if((wp&0xfff0)==SC_KEYMENU||(wp&0xfff0)==SC_MOUSEMENU){RECT r;GetWindowRect(w,&r);POINT point{r.left+8,r.top+32};if((wp&0xfff0)==SC_MOUSEMENU)GetCursorPos(&point);visual::windowMenu(w,point,darkMode(),prefs.english!=0);return 0;}break;
 case WM_NCRBUTTONUP:if(wp==HTCAPTION||wp==HTSYSMENU){POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};visual::windowMenu(w,point,darkMode(),prefs.english!=0);return 0;}break;
 case WM_NCCALCSIZE:if(wp)return 0;break;
 case WM_NCPAINT:return 0;case WM_NCACTIVATE:return TRUE;
 case WM_PAINT:drawDialog(w,*d);return 0;case WM_ERASEBKGND:return 1;
 case WM_CTLCOLOREDIT:{auto dc=HDC(wp);SetTextColor(dc,darkMode()?RGB(242,242,241):RGB(23,25,28));SetBkColor(dc,darkMode()?RGB(16,16,16):RGB(250,250,250));return LRESULT(d->background);}
 case WM_NCHITTEST:{POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(w,&point);float scale=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);if(point.y<52*scale&&point.x<r.right-50*scale)return HTCAPTION;return HTCLIENT;}
 case WM_GETMINMAXINFO:{RECT r;GetWindowRect(w,&r);auto info=reinterpret_cast<MINMAXINFO*>(lp);info->ptMinTrackSize={r.right-r.left,r.bottom-r.top};info->ptMaxTrackSize=info->ptMinTrackSize;return 0;}
 case WM_MOUSEMOVE:{float scale=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/scale;
  if(d->draggingNameScroll){auto track=fileScrollTrack(*d),thumb=fileScrollThumb(*d);float travel=std::max(1.f,(track.right-track.left)-(thumb.right-thumb.left));d->horizontal=std::clamp(d->dragStartOffset+(x-d->dragStartX)*d->horizontalMax/travel,0.f,d->horizontalMax);InvalidateRect(w,nullptr,FALSE);return 0;}
  int h=dialogHit(*d,x,GET_Y_LPARAM(lp)/scale);if(h!=d->hover){d->hover=h;InvalidateRect(w,nullptr,FALSE);}SetCursor(LoadCursorW(nullptr,h>=0?IDC_HAND:IDC_ARROW));return 0;}
 case WM_LBUTTONDOWN:{if(d->kind==2&&d->horizontalMax>0){float scale=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/scale,y=GET_Y_LPARAM(lp)/scale;auto track=fileScrollTrack(*d);if(x>=track.left&&x<=track.right&&y>=d->canvas.height-94&&y<d->canvas.height-71){auto thumb=fileScrollThumb(*d);if(x<thumb.left||x>thumb.right){float travel=std::max(1.f,(track.right-track.left)-(thumb.right-thumb.left));d->horizontal=std::clamp((x-track.left-(thumb.right-thumb.left)/2.f)*d->horizontalMax/travel,0.f,d->horizontalMax);}d->dragStartX=x;d->dragStartOffset=d->horizontal;d->draggingNameScroll=true;SetCapture(w);InvalidateRect(w,nullptr,FALSE);return 0;}}break;}
 case WM_CAPTURECHANGED:d->draggingNameScroll=false;return 0;
 case WM_LBUTTONUP:{if(d->draggingNameScroll){d->draggingNameScroll=false;ReleaseCapture();InvalidateRect(w,nullptr,FALSE);return 0;}float scale=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/scale,y=GET_Y_LPARAM(lp)/scale;int id=dialogHit(*d,x,y);d->keyboardFocus=(wp&0x10000)!=0;if(id>=11&&id<=17){d->focused=id;SetFocus(w);}
 if(id==1)acceptDialog(w,*d);else if(id==2)DestroyWindow(w);else if(id==11){auto folder=textOf(GetDlgItem(w,10));if(browse(w,folder))SetWindowTextW(GetDlgItem(w,10),folder.c_str());}else if(id>=13&&id<=17){if(id<=15)d->draft.dark=id==13?2:id==14?0:1;else d->draft.english=id-16;liveSettings(w,*d);}else if(id==12){POINT point{int((d->canvas.width-250)*scale),int(203*scale)};ClientToScreen(w,&point);int choice=visual::menu(w,point,{{1,tr(L"Начать загрузку",L"Start download"),d->draft.action==0},{2,tr(L"Выбрать файлы",L"Choose files"),d->draft.action==1}},darkMode());if(choice){d->draft.action=choice-1;liveSettings(w,*d);}}



 else if(d->kind==2){
  if(id==20||id==21)sortDialogFiles(*d,id==20?1:2);
  else if(y>=112&&y<143&&x<60){bool all=std::all_of(d->priorities.begin(),d->priorities.end(),[](int p){return p>0;});for(int i=0;i<int(d->priorities.size());++i){int& p=d->priorities[i];if(all){d->remembered[i]=p;p=0;}else if(!p)p=d->remembered[i];}}
  else if(y>=143&&y<(d->horizontalMax>0?d->canvas.height-96:d->canvas.height-71)){int row=int(y-143+d->offset)/42;if(row>=0&&row<int(d->priorities.size())){int i=fileIndex(*d,row);d->focused=100+row;if(x<60){if(d->priorities[i]){d->remembered[i]=d->priorities[i];d->priorities[i]=0;}else d->priorities[i]=d->remembered[i];}else if(x>d->canvas.width-150){int values[]={0,1,4,7};std::vector<visual::MenuItem> entries;for(int v=0;v<4;++v)entries.push_back({v+1,priorityLabel(values[v]),d->priorities[i]==values[v]});POINT point{int((d->canvas.width-150)*scale),int((143+row*42-d->offset+36)*scale)};ClientToScreen(w,&point);int choice=visual::menu(w,point,std::move(entries),darkMode());if(choice){if(d->priorities[i])d->remembered[i]=d->priorities[i];d->priorities[i]=values[choice-1];}}}}
  InvalidateRect(w,nullptr,FALSE);return 0;}
 InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_MOUSEWHEEL:if(d->kind==2){if(GET_KEYSTATE_WPARAM(wp)&MK_SHIFT)d->horizontal=std::clamp(d->horizontal-GET_WHEEL_DELTA_WPARAM(wp)/3.f,0.f,d->horizontalMax);else d->offset=std::clamp(d->offset-GET_WHEEL_DELTA_WPARAM(wp)/3,0,maxFilesScroll(*d));InvalidateRect(w,nullptr,FALSE);}return 0;
 case WM_MOUSEHWHEEL:if(d->kind==2){d->horizontal=std::clamp(d->horizontal+GET_WHEEL_DELTA_WPARAM(wp)/3.f,0.f,d->horizontalMax);InvalidateRect(w,nullptr,FALSE);}return 0;
 case WM_COMMAND:if(d->kind==1&&LOWORD(wp)==10&&HIWORD(wp)==EN_CHANGE){liveSettings(w,*d);return 0;}if(LOWORD(wp)==1)acceptDialog(w,*d);if(LOWORD(wp)==2)DestroyWindow(w);return 0;
 case WM_TIMER:if(wp==99){KillTimer(w,99);visual::capture(w,d->kind==2?L"files.png":L"settings.png");DestroyWindow(w);return 0;}if(d->kind==2&&d->handle.torrent_file()){fillFiles(*d);KillTimer(w,2);InvalidateRect(w,nullptr,FALSE);}return 0;
 case WM_CLOSE:DestroyWindow(w);return 0;case WM_DESTROY:d->done=true;DeleteObject(d->font);DeleteObject(d->background);return 0;
 }return DefWindowProcW(w,m,wp,lp);}
static bool dialog(Dialog& d){EnableWindow(mainWindow,FALSE);float scale=GetDpiForWindow(mainWindow)/96.f;int width=d.kind==2?580:490,height=d.kind==2?std::clamp(213+(d.fixture?5:d.handle.torrent_file()?d.handle.torrent_file()->num_files():5)*42,300,590):d.kind==1?335:215;RECT r;GetWindowRect(mainWindow,&r);HWND w=CreateWindowExW(0,L"TorrentDialog",d.kind==2?tr(L"Файлы торрента",L"Torrent files").c_str():d.kind==1?tr(L"Настройки",L"Settings").c_str():d.kind==3?tr(L"Ошибка",L"Error").c_str():L"Magnet",WS_POPUP|WS_THICKFRAME|WS_SYSMENU|WS_CLIPCHILDREN,r.left+(r.right-r.left-int(width*scale))/2,r.top+(r.bottom-r.top-int(height*scale))/2,int(width*scale),int(height*scale),mainWindow,nullptr,instance,&d);ShowWindow(w,SW_SHOW);MSG m;while(!d.done&&GetMessageW(&m,nullptr,0,0)>0){if(m.message==WM_KEYDOWN&&m.wParam==VK_TAB){std::vector<int> ids=d.kind==1?std::vector<int>{10,11,12,13,14,15,16,17}:d.kind==0?std::vector<int>{10,2,1}:std::vector<int>{100,2,1};auto found=std::find(ids.begin(),ids.end(),d.focused);int at=found==ids.end()?0:int(found-ids.begin());at=(at+((GetKeyState(VK_SHIFT)&0x8000)?int(ids.size())-1:1))%int(ids.size());d.focused=ids[at];d.keyboardFocus=true;SetFocus(d.focused==10?GetDlgItem(w,10):w);InvalidateRect(w,nullptr,FALSE);continue;}
 if(m.message==WM_KEYDOWN&&d.kind==2&&d.focused>=100){int row=d.focused-100,i=fileIndex(d,row);if(m.wParam==VK_DOWN||m.wParam==VK_UP){row=std::clamp(row+(m.wParam==VK_DOWN?1:-1),0,std::max(0,int(d.priorities.size())-1));d.focused=100+row;d.offset=std::clamp(row*42-84,0,maxFilesScroll(d));}else if((m.wParam==VK_LEFT||m.wParam==VK_RIGHT)&&(GetKeyState(VK_SHIFT)&0x8000)){d.horizontal=std::clamp(d.horizontal+(m.wParam==VK_RIGHT?40.f:-40.f),0.f,d.horizontalMax);}else if(i<int(d.priorities.size())&&m.wParam==VK_SPACE){if(d.priorities[i]){d.remembered[i]=d.priorities[i];d.priorities[i]=0;}else d.priorities[i]=d.remembered[i];}else if(i<int(d.priorities.size())&&(m.wParam==VK_LEFT||m.wParam==VK_RIGHT)){int values[]={0,1,4,7};int at=0;while(at<3&&d.priorities[i]>values[at])++at;d.priorities[i]=values[(at+(m.wParam==VK_RIGHT?1:3))%4];}else {TranslateMessage(&m);DispatchMessageW(&m);}InvalidateRect(w,nullptr,FALSE);continue;}
 if(m.message==WM_KEYDOWN&&(m.wParam==VK_SPACE||m.wParam==VK_RETURN)&&d.focused!=10){if(d.focused==2)DestroyWindow(w);else if(d.focused==1)acceptDialog(w,d);else {auto r=settingRect(d.focused,d.canvas.width);float x=d.focused==11?d.canvas.width-40:(r.left+r.right)/2,y=d.focused==11?120.f:(r.top+r.bottom)/2;PostMessageW(w,WM_LBUTTONUP,0x10000,MAKELPARAM(int(x*scale),int(y*scale)));}continue;}
 if(m.message==WM_KEYDOWN&&m.wParam==VK_ESCAPE){DestroyWindow(w);continue;}if(m.message==WM_KEYDOWN&&m.wParam==VK_RETURN){acceptDialog(w,d);continue;}TranslateMessage(&m);DispatchMessageW(&m);}EnableWindow(mainWindow,TRUE);SetForegroundWindow(mainWindow);mainFocused=-1;mainKeyboardFocus=false;InvalidateRect(mainWindow,nullptr,FALSE);return d.accepted;}
static void error(std::string const& message){if(!mainWindow){MessageBoxW(nullptr,wide(message).c_str(),L"Torrent",MB_OK|MB_ICONERROR);return;}Dialog d{3};d.result=wide(message);dialog(d);}
static void refreshTheme(){visual::frame(mainWindow,darkMode(),true);InvalidateRect(mainWindow,nullptr,FALSE);EnumThreadWindows(GetCurrentThreadId(),[](HWND w,LPARAM)->BOOL{wchar_t name[64];GetClassNameW(w,name,64);if(std::wstring(name)==L"TorrentDialog"){auto d=reinterpret_cast<Dialog*>(GetWindowLongPtrW(w,GWLP_USERDATA));if(d){DeleteObject(d->background);d->background=CreateSolidBrush(darkMode()?RGB(16,16,16):RGB(250,250,250));visual::frame(w,darkMode(),true);InvalidateRect(w,nullptr,FALSE);InvalidateRect(GetDlgItem(w,10),nullptr,TRUE);}}return TRUE;},0);}
static void add(std::wstring const& source) {try {auto h=engine->add(utf8(source),utf8(prefs.folder),prefs.action==1); if(prefs.action==1&&h.torrent_file()) {for(auto& item:engine->items)if(item.handle==h)item.selectionShown=true;Dialog d{2};d.handle=h;dialog(d);} InvalidateRect(mainWindow,nullptr,FALSE);} catch(std::exception const& e) {error(e.what());}}
static void openTorrent() {wchar_t file[32768]{}; OPENFILENAMEW o{}; o.lStructSize=sizeof(o); o.hwndOwner=mainWindow; o.lpstrFilter=L"Torrent\0*.torrent\0"; o.lpstrFile=file; o.nMaxFile=32768; o.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR; if(GetOpenFileNameW(&o)) add(file);}

static visual::Canvas mainCanvas;
static void paint(HWND);
static bool settingsOpen=false,settingsClosing=false,settingsSaveFailed=false;
static int settingsHover=-1,settingsFocus=0;
static ULONGLONG settingsStarted=0;
static float settingsFrom=0,settingsProgress=0;
static visual::SettingsBackdrop settingsBackdrop;
static constexpr UINT_PTR settingsTimer=0xc013;
static D2D1_RECT_F settingsSheet(){float width=std::min(720.f,mainCanvas.width-96.f),height=std::min(560.f,mainCanvas.height-56.f);float left=(mainCanvas.width-width)/2;return D2D1::RectF(left,0,left+width,height);}
static D2D1_RECT_F overlayControl(int id){auto r=settingsSheet();float right=r.right-28;switch(id){case 0:return D2D1::RectF(right-22,16,right+10,48);case 1:case 2:case 3:{float x=right-270+(id-1)*90;return D2D1::RectF(x,100,x+90,134);}case 4:return D2D1::RectF(right-270,184,right,220);case 5:return D2D1::RectF(right-90,274,right,310);case 6:return D2D1::RectF(right-270,320,right,356);case 7:return D2D1::RectF(right-112,466,right,502);case 8:{float center=(r.left+r.right)/2;return D2D1::RectF(center-44,r.bottom-32,center+44,r.bottom-8);}}return {};}
static int overlayHit(float x,float y){if(settingsProgress<.99f)return -1;for(int id=0;id<9;++id)if(visual::hit(x,y,overlayControl(id)))return id;return -1;}
static void animateSettings(bool close){settingsClosing=close;settingsFrom=settingsProgress;settingsStarted=GetTickCount64();BOOL motion=TRUE;SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,&motion,0);if(!motion){settingsProgress=close?0.f:1.f;settingsOpen=!close;}else SetTimer(mainWindow,settingsTimer,16,nullptr);InvalidateRect(mainWindow,nullptr,FALSE);}
static void openSettings(){if(settingsOpen)return;paint(mainWindow);DwmFlush();settingsBackdrop.capture(mainWindow,darkMode());settingsOpen=true;settingsFocus=0;settingsHover=-1;settingsProgress=0;animateSettings(false);SetFocus(mainWindow);}
static void recaptureSettings(){bool opened=settingsOpen;settingsOpen=false;paint(mainWindow);DwmFlush();settingsBackdrop.capture(mainWindow,darkMode());settingsOpen=opened;InvalidateRect(mainWindow,nullptr,FALSE);}
static void activateSetting(int id){if(id==0){animateSettings(true);return;}auto before=prefs;if(id>=1&&id<=3)prefs.dark=id==1?2:id==2?0:1;
 if(id==5){auto folder=prefs.folder;if(browse(mainWindow,folder))prefs.folder=folder;}
 if(id==4||id==6){auto r=overlayControl(id);POINT point{int(r.left*mainCanvas.scale),int(r.bottom*mainCanvas.scale)};ClientToScreen(mainWindow,&point);int choice=id==4?visual::menu(mainWindow,point,{{1,L"Русский",prefs.english==0},{2,L"English",prefs.english==1}},darkMode()):visual::menu(mainWindow,point,{{1,tr(L"Начать загрузку",L"Start download"),prefs.action==0},{2,tr(L"Выбрать файлы",L"Choose files"),prefs.action==1}},darkMode());if(choice){if(id==4)prefs.english=choice-1;else prefs.action=choice-1;}}
 if(id==8){ShellExecuteW(mainWindow,L"open",L"https://connect.sanderstripa.com/?utm_source=chatgpt.com",nullptr,nullptr,SW_SHOWNORMAL);return;}
 if(id==7){ShellExecuteW(mainWindow,L"open",L"https://github.com/sanderstripa/torrent/releases/latest",nullptr,nullptr,SW_SHOWNORMAL);return;}
 settingsSaveFailed=!savePrefs();if(settingsSaveFailed)prefs=before;visual::frame(mainWindow,darkMode(),true);recaptureSettings();}
static void drawSettings(){auto& c=mainCanvas;settingsBackdrop.draw(c);auto r=settingsSheet();float offset=-(r.bottom+32)*(1-settingsProgress);c.rt->SetTransform(D2D1::Matrix3x2F::Translation(0,offset));
 // Layered translucent silhouettes approximate a diffuse elevation shadow.
 for(int i=24;i>0;--i){c.color(0);c.ink->SetOpacity(c.dark?.012f:.009f);auto shadow=D2D1::RoundedRect(D2D1::RectF(r.left-i*.7f,8,r.right+i*.7f,r.bottom+6+i),14,14);c.rt->FillRoundedRectangle(shadow,c.ink.Get());}c.ink->SetOpacity(1);
 UINT surface=c.dark?0x202124:0xfcfcfd,field=c.dark?0x28292d:0xf3f4f5,line=c.dark?0x35363b:0xe6e7e9;
 c.box(r,surface,14);c.box(D2D1::RectF(r.left,0,r.right,20),surface,0);
 float left=r.left+28,right=r.right-28;c.label(tr(L"Настройки",L"Settings"),D2D1::RectF(left,13,right-40,52),20,true);
 auto close=overlayControl(0);if(settingsHover==0)c.box(close,c.dark?0x54242f:0xf6d1d9,16);c.close(close.left+11,close.top+11,settingsHover==0);
 auto heading=[&](std::wstring const& text,float y){c.label(text,D2D1::RectF(left,y,right,y+28),13,true);c.line(left,y+31,right,y+31,line,1);};
 heading(tr(L"Внешний вид",L"Appearance"),60);c.label(tr(L"Тема",L"Theme"),D2D1::RectF(left,100,right-280,134),13);
 c.box(D2D1::RectF(right-273,97,right+3,137),field,8,line);for(int id=1;id<=3;++id){auto b=overlayControl(id);bool chosen=prefs.dark==(id==1?2:id==2?0:1);if(chosen)c.box(D2D1::RectF(b.left+2,b.top+1,b.right-2,b.bottom-1),surface,6,line);else if(settingsHover==id)c.box(b,c.selected(),6);c.label(id==1?tr(L"Системная",L"System"):id==2?tr(L"Светлая",L"Light"):tr(L"Тёмная",L"Dark"),b,12,false,0,DWRITE_TEXT_ALIGNMENT_CENTER);}
 heading(tr(L"Язык интерфейса",L"Interface language"),146);auto lang=overlayControl(4);c.box(lang,field,7,line);c.label(prefs.english?L"English":L"Русский",D2D1::RectF(lang.left+12,lang.top,lang.right-30,lang.bottom),13);c.chevron(lang.right-22,lang.top+15);
 heading(tr(L"Загрузки",L"Downloads"),234);c.label(tr(L"Папка загрузок",L"Download folder"),D2D1::RectF(left,274,right-280,310),13);c.box(D2D1::RectF(right-270,274,right-100,310),field,7,line);c.label(prefs.folder,D2D1::RectF(right-260,274,right-110,310),12,false,c.muted());c.button(tr(L"Выбрать",L"Choose"),overlayControl(5),false,settingsHover==5);
 c.label(tr(L"При добавлении торрента",L"On adding a torrent"),D2D1::RectF(left,320,right-280,356),13);auto action=overlayControl(6);c.box(action,field,7,line);c.label(prefs.action?tr(L"Выбрать файлы",L"Choose files"):tr(L"Начать загрузку",L"Start download"),D2D1::RectF(action.left+12,action.top,action.right-30,action.bottom),12);c.chevron(action.right-22,action.top+15);
 heading(tr(L"О Torrent",L"About Torrent"),370);c.label(tr(L"Версия Torrent",L"Torrent version"),D2D1::RectF(left,405,right-150,432),13);c.label(L"0.3.5",D2D1::RectF(right-150,405,right,432),13,false,0,DWRITE_TEXT_ALIGNMENT_TRAILING);c.line(left,434,right,434,line,1);c.label(tr(L"Разработчик",L"Developer"),D2D1::RectF(left,436,right-180,463),13);c.label(L"Sander Stripa",D2D1::RectF(right-180,436,right,463),13,false,c.muted(),DWRITE_TEXT_ALIGNMENT_TRAILING);c.button(tr(L"Обновить",L"Update"),overlayControl(7),false,settingsHover==7);
 if(settingsSaveFailed)c.label(tr(L"Не удалось сохранить настройки",L"Could not save settings"),D2D1::RectF(left,504,right,524),12,false,0xc95050);
 c.label(L"by connect.",overlayControl(8),11,false,settingsHover==8?(c.dark?0xb6b7bd:0x55585d):c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);
 if(mainKeyboardFocus&&settingsFocus>=0)c.focus(overlayControl(settingsFocus));c.rt->SetTransform(D2D1::Matrix3x2F::Identity());
}

static WindowPosition windowPosition;
static lt::torrent_handle selectedTorrent;
static int selectedIndex(){for(int i=0;i<int(engine->items.size());++i)if(engine->items[i].handle==selectedTorrent)return i;return -1;}
static int rowAt(int y){int row=(y-86+scroll)/136;return y>=86&&row>=0&&row<int(engine->items.size())&&(y-86+scroll)%136<124?row:-1;}
static void paint(HWND w){PAINTSTRUCT ps;BeginPaint(w,&ps);auto& c=mainCanvas;c.dark=darkMode();
 if(c.begin(w)){float width=c.width;
 c.label(L"TORRENT.",D2D1::RectF(24,0,width-190,62),visual::ui::title,false,0,DWRITE_TEXT_ALIGNMENT_LEADING,1.6f);
 for(int id:{0,2,3}){float cx=width-(id==0?110:id==2?70:30);c.caption(id==0?0:id==2?1:2,cx,32,mainHover==id);if(mainKeyboardFocus&&mainFocused==id&&GetFocus()==w)c.focus(D2D1::RectF(cx-16,16,cx+16,48));}
 if(designPreview){const wchar_t* names[]={L"Ubuntu Desktop.iso",L"Linux Mint.iso",L"The Witcher S01"};const wchar_t* details[]={L"3,2 ГБ · Загрузка",L"2,8 ГБ · Завершено · Раздача",L"12,6 ГБ · Пауза"};float progress[]={.72f,1.f,.43f};for(int i=0;i<3;++i){float y=86.f+i*136;c.box(D2D1::RectF(18,y,width-18,y+124),i==2?c.selected():c.card(),11,c.lineColor());c.label(names[i],D2D1::RectF(34,y+12,width-110,y+42),17,true);c.label(details[i],D2D1::RectF(34,y+43,width-110,y+68),15,false,c.muted());if(i==1)c.success(width-51,y+42);else c.label(std::to_wstring(int(progress[i]*100))+L"%",D2D1::RectF(width-93,y+30,width-34,y+58),17,false,0,DWRITE_TEXT_ALIGNMENT_TRAILING);c.box(D2D1::RectF(34,y+79,width-34,y+83),c.track(),4);c.box(D2D1::RectF(34,y+79,34+(width-68)*progress[i],y+83),c.fg(),4);c.label(i==0?L"Осталось 1 мин 12 сек":i==1?L"100%":L"Пауза",D2D1::RectF(width-310,y+92,width-34,y+116),visual::ui::help,false,c.muted(),DWRITE_TEXT_ALIGNMENT_TRAILING);}}else if(engine->items.empty()){c.label(tr(L"Добавьте первый торрент",L"Add your first torrent"),D2D1::RectF(24,c.height/2-35,width-24,c.height/2),20,true,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);c.label(tr(L"Нажмите + или перетащите сюда .torrent",L"Click + or drop a .torrent file here"),D2D1::RectF(24,c.height/2,width-24,c.height/2+32),15,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 c.rt->PushAxisAlignedClip(D2D1::RectF(0,78,width,c.height),D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
 for(int i=0;i<int(engine->items.size());++i){auto const& item=engine->items[i];auto s=item.handle.status();float y=86.f+i*136-scroll;if(y+124<78||y>c.height)continue;
 bool paused=(s.flags&lt::torrent_flags::paused)&&!(s.flags&lt::torrent_flags::auto_managed);
 std::wstring state=s.errc?tr(L"Ошибка",L"Error"):item.stopped?tr(L"Остановлено",L"Stopped"):item.selectPending?tr(L"Выбор файлов",L"Choose files"):paused?tr(L"Пауза",L"Paused"):!item.handle.torrent_file()?tr(L"Получение списка файлов",L"Fetching metadata"):s.is_seeding?tr(L"Завершено · Раздача",L"Complete · Seeding"):s.state==lt::torrent_status::checking_files?tr(L"Проверка",L"Checking"):(s.flags&lt::torrent_flags::paused)?tr(L"В очереди",L"Queued"):tr(L"Загрузка",L"Downloading");
 c.box(D2D1::RectF(18,y,width-18,y+124),(item.handle==selectedTorrent||paused)?(c.selected()):c.card(),11,item.handle==selectedTorrent?c.fg():c.lineColor());c.label(wide(s.name),D2D1::RectF(34,y+12,width-110,y+42),17,true);c.label(sizeText(item.selectPending&&item.handle.torrent_file()?item.handle.torrent_file()->total_size():s.total_wanted)+L" · "+state,D2D1::RectF(34,y+43,width-110,y+68),15,false,c.muted());
 if(!item.selectPending&&s.is_seeding)c.success(width-51,y+42);else if(!item.selectPending)c.label(std::to_wstring(int(s.progress*100))+L"%",D2D1::RectF(width-93,y+30,width-34,y+58),17,false,0,DWRITE_TEXT_ALIGNMENT_TRAILING);
 c.box(D2D1::RectF(34,y+79,width-34,y+83),c.track(),4);if(!item.selectPending&&s.progress>0)c.box(D2D1::RectF(34,y+79,34+(width-68)*s.progress,y+83),c.fg(),4);
 std::wstring eta=item.selectPending?L"":paused?tr(L"Пауза",L"Paused"):s.is_finished?L"100%":L"";if(!item.selectPending&&s.download_payload_rate>0&&!s.is_finished&&!paused){auto seconds=(s.total_wanted-s.total_wanted_done)/s.download_payload_rate;eta=tr(L"Осталось ",L"Remaining ")+std::to_wstring(seconds/60)+tr(L" мин ",L" min ")+std::to_wstring(seconds%60)+tr(L" сек",L" sec");}c.label(eta,D2D1::RectF(width-310,y+92,width-34,y+116),visual::ui::help,false,c.muted(),DWRITE_TEXT_ALIGNMENT_TRAILING);}
 c.rt->PopAxisAlignedClip();if(settingsOpen)drawSettings();c.end();}EndPaint(w,&ps);}
static void context(int row,POINT p) {std::vector<visual::MenuItem> entries;if(row<0){entries={{1,tr(L"Открыть .torrent",L"Open .torrent")},{2,tr(L"Добавить magnet",L"Add magnet")},{9,tr(L"Настройки",L"Settings")}};}else {entries={{3,tr(L"Пауза",L"Pause")},{4,tr(L"Возобновить",L"Resume")},{5,tr(L"Остановить",L"Stop")},{6,tr(L"Удалить из списка",L"Remove from list")},{0,L""},{7,tr(L"Приоритет выше",L"Higher priority")},{8,tr(L"Приоритет ниже",L"Lower priority")}};}int id=visual::menu(mainWindow,p,std::move(entries),darkMode());
 if(id==9){openSettings();return;}if(id==1) openTorrent(); if(id==2) {Dialog d{0};if(dialog(d)&&!d.result.empty()) add(d.result);} if(row>=0&&id>=3) {auto& item=engine->items[row];auto h=item.handle; if(id==3||id==5) {h.unset_flags(lt::torrent_flags::auto_managed);h.pause(); item.stopped=id==5;} if(id==4) {item.stopped=false; if(!item.selectPending) h.set_flags(lt::torrent_flags::auto_managed);h.resume();} if(id==6) engine->remove(h); if(id==7) {h.queue_position_up();if(row>0) std::swap(engine->items[row],engine->items[row-1]);} if(id==8) {h.queue_position_down();if(row+1<int(engine->items.size())) std::swap(engine->items[row],engine->items[row+1]);} engine->save();} InvalidateRect(mainWindow,nullptr,FALSE);
}
static LRESULT CALLBACK windowProc(HWND w,UINT m,WPARAM wp,LPARAM lp) {
 if(mainCanvas.animationMessage(w,m,wp))return 0;
 try {
 switch(m) {
 case WM_CREATE: visual::frame(w,darkMode(),true);{UINT dpi=GetDpiForWindow(w);HICON titleIcon=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXSMICON,dpi),GetSystemMetricsForDpi(SM_CYSMICON,dpi),LR_DEFAULTCOLOR));SendMessageW(w,WM_SETICON,ICON_SMALL,LPARAM(titleIcon));HICON taskbarIcon=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXICON,dpi),GetSystemMetricsForDpi(SM_CYICON,dpi),LR_DEFAULTCOLOR));SendMessageW(w,WM_SETICON,ICON_BIG,LPARAM(taskbarIcon));}mainWindow=w;SetTimer(w,1,1000,nullptr);DragAcceptFiles(w,TRUE);return 0;
 case WM_SYSCOMMAND:if((wp&0xfff0)==SC_MAXIMIZE)return 0;if((wp&0xfff0)==SC_KEYMENU||(wp&0xfff0)==SC_MOUSEMENU){RECT r;GetWindowRect(w,&r);POINT point{r.left+8,r.top+32};if((wp&0xfff0)==SC_MOUSEMENU)GetCursorPos(&point);visual::windowMenu(w,point,darkMode(),prefs.english!=0);return 0;}break;
 case WM_NCRBUTTONUP:if(wp==HTCAPTION||wp==HTSYSMENU){POINT point{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};context(-1,point);return 0;}break;
 case WM_NCCALCSIZE:if(wp)return 0;break;
 case WM_NCPAINT:return 0;case WM_NCACTIVATE:return TRUE;
 case WM_NCLBUTTONDBLCLK:if(wp==HTCAPTION)return 0;break;
 case WM_SETTINGCHANGE:case WM_THEMECHANGED:readWindowsTheme();if(prefs.dark==2){refreshTheme();if(settingsOpen)recaptureSettings();}return 0;
 case WM_NCHITTEST:{if(settingsOpen)return HTCLIENT;POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(w,&p);RECT r;GetClientRect(w,&r);float scale=GetDpiForWindow(w)/96.f;int edge=int(6*scale);bool left=p.x<edge,right=p.x>=r.right-edge,top=p.y<edge,bottom=p.y>=r.bottom-edge;if(top&&left)return HTTOPLEFT;if(top&&right)return HTTOPRIGHT;if(bottom&&left)return HTBOTTOMLEFT;if(bottom&&right)return HTBOTTOMRIGHT;if(left)return HTLEFT;if(right)return HTRIGHT;if(top)return HTTOP;if(bottom)return HTBOTTOM;if(p.y<62*scale&&p.x<r.right-142*scale)return HTCAPTION;return HTCLIENT;}
 case WM_SETCURSOR:if(settingsOpen&&LOWORD(lp)==HTCLIENT){POINT p;GetCursorPos(&p);ScreenToClient(w,&p);float scale=GetDpiForWindow(w)/96.f;SetCursor(LoadCursorW(nullptr,overlayHit(p.x/scale,p.y/scale)==8?IDC_HAND:IDC_ARROW));return TRUE;}break;
 case WM_MOUSEMOVE:{float scale=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);if(settingsOpen){int h=overlayHit(GET_X_LPARAM(lp)/scale,GET_Y_LPARAM(lp)/scale);if(h!=settingsHover){settingsHover=h;InvalidateRect(w,nullptr,FALSE);}SetCursor(LoadCursorW(nullptr,h==8?IDC_HAND:IDC_ARROW));TRACKMOUSEEVENT track{sizeof track,TME_LEAVE,w,0};TrackMouseEvent(&track);return 0;}int h=mainHit(GET_X_LPARAM(lp)/scale,GET_Y_LPARAM(lp)/scale,r.right/scale);if(h!=mainHover){mainHover=h;InvalidateRect(w,nullptr,FALSE);}TRACKMOUSEEVENT track{sizeof track,TME_LEAVE,w,0};TrackMouseEvent(&track);return 0;}
 case WM_MOUSELEAVE:settingsHover=-1;mainHover=-1;InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_PAINT:paint(w);return 0;
 case WM_APP+1:if(interactionTesting)openSettings();return 0;
 case WM_APP+5:if(interactionTesting)error("UI validation notice");return 0;
 case WM_APP+4:if(interactionTesting){themeTestOverride=wp?1:0;readWindowsTheme();if(prefs.dark==2){refreshTheme();if(settingsOpen)recaptureSettings();}}return 0;
 case WM_APP+2:if(interactionTesting)add((std::filesystem::current_path()/L"pending.torrent").wstring());return 0;
 case WM_APP+3:if(interactionTesting){int row=selectedIndex();if(wp==0)return LRESULT(engine->items.size());if(row<0)return -1;auto flags=selectedTorrent.status().flags;return ((flags&lt::torrent_flags::paused)&&!(flags&lt::torrent_flags::auto_managed))?1:0;}return 0;
 case WM_ERASEBKGND:return 1;
 case WM_GETMINMAXINFO: {auto info=reinterpret_cast<MINMAXINFO*>(lp);float dpi=GetDpiForWindow(w)/96.f;info->ptMinTrackSize={int(740*dpi),int(640*dpi)};return 0;}
 case WM_WINDOWPOSCHANGED:windowPosition.moved(w);break;
 case WM_EXITSIZEMOVE:windowPosition.save(w);return 0;
 case WM_QUERYENDSESSION:windowPosition.save(w);return TRUE;
 case WM_ENDSESSION:if(wp)windowPosition.save(w);return 0;
 case WM_SIZE:settingsBackdrop.reset();InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_DPICHANGED: {auto r=reinterpret_cast<RECT*>(lp);SetWindowPos(w,nullptr,r->left,r->top,r->right-r->left,r->bottom-r->top,SWP_NOZORDER);mainCanvas.rt.Reset();mainCanvas.ink.Reset();HICON titleIcon=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXSMICON,HIWORD(wp)),GetSystemMetricsForDpi(SM_CYSMICON,HIWORD(wp)),LR_DEFAULTCOLOR));HICON previous=HICON(SendMessageW(w,WM_SETICON,ICON_SMALL,LPARAM(titleIcon)));if(previous)DestroyIcon(previous);HICON taskbarIcon=HICON(LoadImageW(instance,MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetricsForDpi(SM_CXICON,HIWORD(wp)),GetSystemMetricsForDpi(SM_CYICON,HIWORD(wp)),LR_DEFAULTCOLOR));previous=HICON(SendMessageW(w,WM_SETICON,ICON_BIG,LPARAM(taskbarIcon)));if(previous)DestroyIcon(previous);return 0;}
 case WM_MOUSEWHEEL: {if(settingsOpen)return 0;RECT r;GetClientRect(w,&r);int viewport=int(r.bottom/(GetDpiForWindow(w)/96.f))-86; scroll=std::clamp(scroll-GET_WHEEL_DELTA_WPARAM(wp)/3,0,std::max(0,int(engine->items.size())*136-viewport));InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_LBUTTONUP:{float scale=GetDpiForWindow(w)/96.f;float x=GET_X_LPARAM(lp)/scale,y=GET_Y_LPARAM(lp)/scale;RECT r;GetClientRect(w,&r);if(settingsOpen){int id=overlayHit(x,y);if(id>=0)activateSetting(id);else if(!visual::hit(x,y,settingsSheet()))animateSettings(true);return 0;}int hit=mainHit(x,y,r.right/scale);mainKeyboardFocus=(wp&0x10000)!=0;mainFocused=mainKeyboardFocus?hit:-1;if(hit==3){SendMessageW(w,WM_CLOSE,0,0);return 0;}if(hit==2){ShowWindow(w,SW_MINIMIZE);return 0;}if(hit==0){POINT p;GetCursorPos(&p);context(-1,p);}else{int row=rowAt(int(y));selectedTorrent=row>=0?engine->items[row].handle:lt::torrent_handle{};}SetFocus(w);InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_LBUTTONDBLCLK: {if(settingsOpen)return 0;int row=rowAt(int(GET_Y_LPARAM(lp)/(GetDpiForWindow(w)/96.f)));if(row>=0){selectedTorrent=engine->items[row].handle;Dialog d{2};d.handle=selectedTorrent;dialog(d);}return 0;}
 case WM_KEYDOWN: {if(settingsOpen){if(wp==VK_ESCAPE)animateSettings(true);else if(wp==VK_TAB){mainKeyboardFocus=true;settingsFocus=(settingsFocus+((GetKeyState(VK_SHIFT)&0x8000)?8:1))%9;InvalidateRect(w,nullptr,FALSE);}else if(wp==VK_RETURN||wp==VK_SPACE)activateSetting(settingsFocus);return 0;}if(wp==VK_F10&&(GetKeyState(VK_SHIFT)&0x8000)){POINT p{24,62};ClientToScreen(w,&p);context(-1,p);return 0;}if(wp==VK_TAB){mainKeyboardFocus=true;int ids[]={0,2,3};int at=mainFocused==2?1:mainFocused==3?2:0;mainFocused=ids[(at+((GetKeyState(VK_SHIFT)&0x8000)?2:1))%3];InvalidateRect(w,nullptr,FALSE);return 0;}if((wp==VK_RETURN||wp==VK_SPACE)&&mainFocused>=0){RECT r;GetClientRect(w,&r);float scale=GetDpiForWindow(w)/96.f;PostMessageW(w,WM_LBUTTONUP,0x10000,MAKELPARAM(int(r.right-(mainFocused==0?110:mainFocused==2?70:30)*scale),int(32*scale)));return 0;}int row=selectedIndex();if(row>=0&&wp==VK_DELETE){engine->remove(selectedTorrent);selectedTorrent=engine->items.empty()?lt::torrent_handle{}:engine->items[std::min(row,int(engine->items.size())-1)].handle;InvalidateRect(w,nullptr,FALSE);return 0;}if(row>=0&&wp==VK_SPACE){if(lp&(1LL<<30))return 0;auto& item=engine->items[row];auto h=item.handle;auto flags=h.status().flags;if(item.stopped||((flags&lt::torrent_flags::paused)&&!(flags&lt::torrent_flags::auto_managed))){item.stopped=false;if(!item.selectPending)h.set_flags(lt::torrent_flags::auto_managed);h.resume();}else{h.unset_flags(lt::torrent_flags::auto_managed);h.pause();}engine->save();InvalidateRect(w,nullptr,FALSE);return 0;}break;}
 case WM_CONTEXTMENU: {if(settingsOpen)return 0;POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)},local=p;ScreenToClient(w,&local);context(rowAt(int(local.y/(GetDpiForWindow(w)/96.f))),p);return 0;}
 case WM_DROPFILES: {HDROP drop=HDROP(wp);UINT count=DragQueryFileW(drop,0xffffffff,nullptr,0);for(UINT i=0;i<count;++i){wchar_t path[32768];DragQueryFileW(drop,i,path,32768);add(path);}DragFinish(drop);return 0;}
 case WM_COPYDATA: {auto c=reinterpret_cast<COPYDATASTRUCT*>(lp);if(c->dwData==1&&c->cbData>=sizeof(wchar_t)&&c->cbData<=65536&&reinterpret_cast<wchar_t*>(c->lpData)[c->cbData/sizeof(wchar_t)-1]==0) add(static_cast<wchar_t*>(c->lpData));SetForegroundWindow(w);return TRUE;}
 case WM_TIMER: {if(wp==settingsTimer){float elapsed=std::min(1.f,float(GetTickCount64()-settingsStarted)/220.f),ease=1-std::pow(1-elapsed,3.f);settingsProgress=settingsFrom+((settingsClosing?0.f:1.f)-settingsFrom)*ease;if(elapsed>=1){KillTimer(w,settingsTimer);if(settingsClosing){settingsOpen=false;settingsBackdrop.reset();}}InvalidateRect(w,nullptr,FALSE);return 0;}if(wp==100){KillTimer(w,100);settingsProgress=1;paint(w);visual::capture(w,L"settings.png");settingsOpen=false;PostMessageW(w,WM_TIMER,101,0);return 0;}if(wp==WindowPosition::timer){windowPosition.save(w);return 0;}if(wp==99){KillTimer(w,99);visual::capture(w,L"main.png");openSettings();SetTimer(w,100,350,nullptr);return 0;}if(wp==101){Dialog files{2};files.fixture=true;files.names={L"01. Серия 1.mkv",L"02. Серия 2.mkv",L"03. Серия 3.mkv",L"04. Постер.jpg",L"05. Readme.txt"};files.sizes={L"1,2 ГБ",L"1,3 ГБ",L"1,3 ГБ",L"3,4 МБ",L"12 КБ"};files.priorities={0,7,7,1,0};files.remembered={4,7,7,1,4};files.bytes={1288490188LL,1395864371LL,1395864371LL,3565158LL,12288LL};dialog(files);PostMessageW(w,WM_CLOSE,0,0);return 0;}engine->tick(); static int seconds=0;if(++seconds%30==0)engine->save();if(!engine->errors.empty()){auto e=engine->errors.back();engine->errors.clear();error(e);}InvalidateRect(w,nullptr,FALSE);if(IsWindowEnabled(w)&&!settingsOpen){for(auto& item:engine->items)if(item.selectPending&&!item.selectionShown&&item.handle.torrent_file()){item.selectionShown=true;Dialog d{2};d.handle=item.handle;dialog(d);break;}}return 0;}
 case WM_CLOSE:if(!closing){windowPosition.save(w);closing=true;KillTimer(w,1);EnableWindow(w,FALSE);engine->shutdown();DestroyWindow(w);}return 0;
 case WM_DESTROY:PostQuitMessage(0);return 0;
 }
 } catch(std::exception const& e) {if(smokeTesting){std::ofstream("Torrent-startup-error.txt")<<e.what();PostQuitMessage(1);return 0;}if(m==WM_CLOSE){closing=false;EnableWindow(w,TRUE);SetTimer(w,1,1000,nullptr);}error(e.what());}
 return DefWindowProcW(w,m,wp,lp);
}
int WINAPI wWinMain(HINSTANCE h,HINSTANCE,PWSTR,int show) {
 instance=h;SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED); InitCommonControls();
 int argc=0;auto argv=CommandLineToArgvW(GetCommandLineW(),&argc);
 interactionTesting=argc==2&&std::wstring(argv[1])==L"--interaction-test";bool darkPreview=argc==2&&std::wstring(argv[1])==L"--visual-test-dark";designPreview=argc==2&&(std::wstring(argv[1])==L"--visual-test"||darkPreview);bool smoke=interactionTesting||designPreview||(argc==2&&std::wstring(argv[1])==L"--smoke-test");
 smokeTesting=smoke;stage("entered wWinMain");
 HANDLE mutex=CreateMutexW(nullptr,FALSE,smoke?L"Local\\Torrent.Native.Client.Test.v1":L"Local\\Torrent.Native.Client.v1"); if(GetLastError()==ERROR_ALREADY_EXISTS) {HWND existing=FindWindowW(L"TorrentMain",nullptr);if(existing){for(int i=1;i<argc;++i){COPYDATASTRUCT c{1,DWORD((wcslen(argv[i])+1)*sizeof(wchar_t)),argv[i]};SendMessageW(existing,WM_COPYDATA,0,LPARAM(&c));}ShowWindow(existing,SW_RESTORE);SetForegroundWindow(existing);}LocalFree(argv);CloseHandle(mutex);CoUninitialize();return 0;}
 try {stage("resolving folders");dataRoot=smoke?std::filesystem::current_path()/(interactionTesting?L"ui-test-data":L"smoke-data"):knownFolder(FOLDERID_LocalAppData)/L"Torrent";prefs.folder=knownFolder(FOLDERID_Downloads).wstring();std::filesystem::create_directories(dataRoot);{std::ifstream f(dataRoot/L"settings.txt",std::ios::binary);std::string folder;if(std::getline(f,folder)){prefs.folder=wide(folder);f>>prefs.action>>prefs.dark>>prefs.english;}}prefs.action=std::clamp(prefs.action,0,1);prefs.dark=std::clamp(prefs.dark,0,2);prefs.english=std::clamp(prefs.english,0,1);if(designPreview)prefs.dark=darkPreview?1:0;readWindowsTheme();stage("starting engine");engine=std::make_unique<Engine>(dataRoot/L"session");stage("initializing drawing");
 WNDCLASSW wc{};wc.hInstance=h;wc.style=CS_DBLCLKS;wc.lpfnWndProc=windowProc;wc.lpszClassName=L"TorrentMain";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);wc.hIcon=LoadIconW(h,MAKEINTRESOURCEW(1));if(!RegisterClassW(&wc))throw std::runtime_error("Main class registration failed: "+std::to_string(GetLastError()));wc.lpfnWndProc=dialogProc;wc.lpszClassName=L"TorrentDialog";wc.hbrBackground=nullptr;if(!RegisterClassW(&wc))throw std::runtime_error("Dialog class registration failed: "+std::to_string(GetLastError()));
 stage("creating window");HWND w=CreateWindowExW(0,L"TorrentMain",L"Torrent",WS_POPUP|WS_THICKFRAME|WS_SYSMENU|WS_MINIMIZEBOX,CW_USEDEFAULT,CW_USEDEFAULT,int(940*GetDpiForSystem()/96.f),int(720*GetDpiForSystem()/96.f),nullptr,nullptr,h,nullptr);if(!w)throw std::runtime_error("Window creation failed: "+std::to_string(GetLastError()));stage("window created");windowPosition.restore(w,dataRoot,!smoke||interactionTesting);ShowWindow(w,(designPreview||interactionTesting)?SW_SHOWNOACTIVATE:show);windowPosition.start(w);if(smoke){paint(w);if(!visual::productFont.collection)throw std::runtime_error("Embedded Montserrat could not be loaded");if(!mainCanvas.rt)throw std::runtime_error("Direct2D target could not be created");stage("Direct2D rendered");if(interactionTesting)PostMessageW(w,WM_APP+1,0,0);else if(designPreview)SetTimer(w,99,250,nullptr);else PostMessageW(w,WM_CLOSE,0,0);}else for(int i=1;i<argc;++i)add(argv[i]);LocalFree(argv);MSG m;while(GetMessageW(&m,nullptr,0,0)>0){TranslateMessage(&m);DispatchMessageW(&m);}engine.reset();if(smoke){stage("clean shutdown");CloseHandle(mutex);CoUninitialize();return int(m.wParam);} } catch(std::exception const& e){if(smoke){std::ofstream("Torrent-startup-error.txt")<<e.what();CloseHandle(mutex);CoUninitialize();return 1;}MessageBoxW(nullptr,wide(e.what()).c_str(),L"Torrent",MB_ICONERROR);}
 CloseHandle(mutex);CoUninitialize();return 0;
}
