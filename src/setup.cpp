#include "visual.h"
#include "popup.h"
#include <windowsx.h>
#include <shellapi.h>
#include <shlobj.h>
#include <filesystem>
#include <fstream>
#include <vector>
#include <wincodec.h>
using Microsoft::WRL::ComPtr;
static visual::Canvas canvas;
static ComPtr<ID2D1Bitmap> icon;
static HINSTANCE instance;static HWND window;
static int page=0,hover=-1;static bool capturePreview=false;static bool launch=true,preview=false;
static HANDLE installer=nullptr;static std::filesystem::path temporary,progressPath;static int progress=0;
static void loadIcon(){ComPtr<IWICImagingFactory> imaging;ComPtr<IWICBitmapDecoder> decoder;ComPtr<IWICBitmapFrameDecode> frame;ComPtr<IWICFormatConverter> converter;
 if(FAILED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&imaging))))return;
 HRSRC res=FindResourceW(instance,MAKEINTRESOURCEW(3),RT_RCDATA);if(!res)return;auto data=LoadResource(instance,res);ComPtr<IWICStream> stream;imaging->CreateStream(&stream);stream->InitializeFromMemory(static_cast<BYTE*>(LockResource(data)),SizeofResource(instance,res));
 if(FAILED(imaging->CreateDecoderFromStream(stream.Get(),nullptr,WICDecodeMetadataCacheOnLoad,&decoder)))return;decoder->GetFrame(0,&frame);imaging->CreateFormatConverter(&converter);converter->Initialize(frame.Get(),GUID_WICPixelFormat32bppPBGRA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom);canvas.rt->CreateBitmapFromWicBitmap(converter.Get(),nullptr,&icon);}
static D2D1_RECT_F primaryRect(){return page==0?D2D1::RectF(40,332,canvas.width-40,378):D2D1::RectF(40,372,canvas.width-40,418);}
static void paint(){PAINTSTRUCT ps;BeginPaint(window,&ps);if(canvas.begin(window)){auto& c=canvas;float w=c.width;c.close(w-27,17);c.line(w-62,22,w-51,22,c.muted());
 if(!icon)loadIcon();if(icon)c.rt->DrawBitmap(icon.Get(),D2D1::RectF((w-136)/2,62,(w+136)/2,198));
 if(page==2){c.color(0xffffff);c.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(w/2+57,177),23,23),c.ink.Get());c.color(0x14c66a);c.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(w/2+57,177),20,20),c.ink.Get());c.line(w/2+47,177,w/2+55,184,0xffffff,2.5f);c.line(w/2+55,184,w/2+68,169,0xffffff,2.5f);}
 c.label(page==0?L"Torrent":page==1?L"Установка":L"Готово",D2D1::RectF(20,223,w-20,265),30,true,0,DWRITE_TEXT_ALIGNMENT_CENTER);
 c.label(page==0?L"Простой торрент-клиент для Windows":page==1?L"Копирование файлов…":L"Приложение успешно установлено",D2D1::RectF(16,271,w-16,300),14,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);
 if(page==0){c.button(L"Установить   →",primaryRect(),true,hover==1,17,8);c.label(L"Отмена",D2D1::RectF(40,395,w-40,427),14,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 if(page==1){c.box(D2D1::RectF(28,338,w-75,346),0xe8e9ec,4);if(progress>0)c.box(D2D1::RectF(28,338,28+(w-103)*progress/100.f,346),0x087cff,4);c.label(std::to_wstring(progress)+L"%",D2D1::RectF(w-64,326,w-22,358),14,false,c.muted());}
 if(page==2){c.check(26,328,launch);c.label(L"Запустить Torrent после установки",D2D1::RectF(52,317,w-20,355),14);c.button(L"Готово",primaryRect(),true,hover==1,17,8);}
 c.end();if(!canvas.rt)icon.Reset();}EndPaint(window,&ps);}
static void startInstall(){if(preview){progress=72;page=1;SetTimer(window,1,40,nullptr);SetTimer(window,2,2000,nullptr);return;}
 wchar_t temp[MAX_PATH];GetTempPathW(MAX_PATH,temp);wchar_t unique[MAX_PATH];GetTempFileNameW(temp,L"Tor",0,unique);temporary=unique;progressPath=temporary;progressPath+=L".progress";
 HRSRC res=FindResourceW(instance,MAKEINTRESOURCEW(2),RT_RCDATA);if(!res){MessageBoxW(window,L"Не найден пакет установки",L"Torrent",MB_ICONERROR);return;}
 {std::ofstream f(temporary,std::ios::binary|std::ios::trunc);f.write(static_cast<char const*>(LockResource(LoadResource(instance,res))),SizeofResource(instance,res));if(!f){MessageBoxW(window,L"Не удалось подготовить установку",L"Torrent",MB_ICONERROR);return;}}
 std::wstring cmd=L"\""+temporary.wstring()+L"\" /S /PROGRESSFILE=\""+progressPath.wstring()+L"\"";STARTUPINFOW si{};si.cb=sizeof si;PROCESS_INFORMATION pi{};
 if(!CreateProcessW(temporary.c_str(),cmd.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,nullptr,&si,&pi)){std::filesystem::remove(temporary);MessageBoxW(window,L"Не удалось запустить установку",L"Torrent",MB_ICONERROR);return;}
 CloseHandle(pi.hThread);installer=pi.hProcess;page=1;SetTimer(window,1,40,nullptr);}
static void finish(){if(page==2&&launch&&!preview){PWSTR folder=nullptr;if(SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData,0,nullptr,&folder))){auto app=std::filesystem::path(folder)/L"Programs"/L"Torrent"/L"Torrent.exe";CoTaskMemFree(folder);ShellExecuteW(window,L"open",app.c_str(),nullptr,nullptr,SW_SHOWNORMAL);}}DestroyWindow(window);}
static LRESULT CALLBACK proc(HWND w,UINT m,WPARAM wp,LPARAM lp){switch(m){case WM_CREATE:window=w;visual::frame(w,false,true);return 0;case WM_SYSCOMMAND:if((wp&0xfff0)==SC_KEYMENU||(wp&0xfff0)==SC_MOUSEMENU){RECT r;GetWindowRect(w,&r);POINT point{r.left+8,r.top+32};visual::windowMenu(w,point,false,false);return 0;}break;case WM_NCCALCSIZE:if(wp)return 0;break;case WM_NCPAINT:return 0;case WM_NCACTIVATE:return TRUE;case WM_PAINT:paint();return 0;case WM_ERASEBKGND:return 1;
 case WM_NCHITTEST:{POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(w,&p);float s=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);if(p.y<60*s&&p.x<r.right-100*s)return HTCAPTION;return HTCLIENT;}
 case WM_MOUSEMOVE:{float s=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/s,y=GET_Y_LPARAM(lp)/s;int h=page!=1&&visual::hit(x,y,primaryRect());if(h!=hover){hover=h;InvalidateRect(w,nullptr,FALSE);}return 0;}
 case WM_LBUTTONUP:{float s=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/s,y=GET_Y_LPARAM(lp)/s;
 if(y<40&&x>canvas.width-42){if(page!=1)finish();}else if(y<40&&x>canvas.width-78)ShowWindow(w,SW_MINIMIZE);
 else if(page==0&&visual::hit(x,y,primaryRect()))startInstall();else if(page==0&&y>=395&&y<427)finish();else if(page==2&&y>=317&&y<355)launch=!launch;else if(page==2&&visual::hit(x,y,primaryRect()))finish();InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_KEYDOWN:if(wp==VK_SPACE&&page==2)launch=!launch;if(wp==VK_RETURN){if(page==0)startInstall();else if(page==2)finish();}if(wp==VK_ESCAPE&&page!=1)finish();InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_TIMER:if(wp==1&&installer){std::ifstream f(progressPath);int value;if(f>>value)progress=std::clamp(value,0,100);}if(wp==99){KillTimer(w,99);const wchar_t* paths[]={L"setup-welcome.png",L"setup-progress.png",L"setup-finish.png"};visual::capture(w,paths[page]);if(++page==3){DestroyWindow(w);return 0;}progress=72;SetTimer(w,99,250,nullptr);}if(wp==2){page=2;KillTimer(w,1);KillTimer(w,2);}if(installer&&WaitForSingleObject(installer,0)==WAIT_OBJECT_0){DWORD code;GetExitCodeProcess(installer,&code);CloseHandle(installer);installer=nullptr;std::error_code ec;std::filesystem::remove(temporary,ec);std::filesystem::remove(progressPath,ec);KillTimer(w,1);page=code?0:2;if(code)MessageBoxW(w,L"Установка не завершена. Попробуйте снова.",L"Torrent",MB_ICONERROR);}InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_DPICHANGED:{auto r=reinterpret_cast<RECT*>(lp);SetWindowPos(w,nullptr,r->left,r->top,r->right-r->left,r->bottom-r->top,SWP_NOZORDER);canvas.rt.Reset();canvas.ink.Reset();icon.Reset();InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_CLOSE:if(page!=1)finish();return 0;case WM_DESTROY:PostQuitMessage(0);return 0;}return DefWindowProcW(w,m,wp,lp);}
int WINAPI wWinMain(HINSTANCE h,HINSTANCE,PWSTR command,int show){instance=h;capturePreview=std::wstring(command)==L"--visual-test";preview=capturePreview||std::wstring(command)==L"--preview";SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);WNDCLASSW wc{};wc.hInstance=h;wc.lpfnWndProc=proc;wc.lpszClassName=L"TorrentSetupVisual";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);wc.hIcon=LoadIconW(h,MAKEINTRESOURCEW(1));RegisterClassW(&wc);float scale=GetDpiForSystem()/96.f;HWND w=CreateWindowExW(0,wc.lpszClassName,L"Torrent Setup",WS_POPUP|WS_THICKFRAME|WS_SYSMENU|WS_MINIMIZEBOX,CW_USEDEFAULT,CW_USEDEFAULT,int(380*scale),int(460*scale),nullptr,nullptr,h,nullptr);RECT area;SystemParametersInfoW(SPI_GETWORKAREA,0,&area,0);SetWindowPos(w,nullptr,area.left+(area.right-area.left-int(380*scale))/2,area.top+(area.bottom-area.top-int(460*scale))/2,0,0,SWP_NOSIZE|SWP_NOZORDER);ShowWindow(w,capturePreview?SW_SHOWNOACTIVATE:show);if(capturePreview)SetTimer(w,99,250,nullptr);MSG m;while(GetMessageW(&m,nullptr,0,0)>0){TranslateMessage(&m);DispatchMessageW(&m);}CoUninitialize();return 0;}
