#include "visual.h"
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
static void paint(){PAINTSTRUCT ps;BeginPaint(window,&ps);if(canvas.begin(window)){auto& c=canvas;float w=c.width;c.gradient(D2D1::RectF(0,0,w,c.height),0xffffff,0xf1f7ff,0);c.close(w-34,22);c.line(w-78,27,w-65,27,c.muted());
 if(!icon)loadIcon();if(icon)c.rt->DrawBitmap(icon.Get(),D2D1::RectF((w-210)/2,95,(w+210)/2,305));
 if(page==2){c.color(0xffffff);c.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(w/2+89,275),32,32),c.ink.Get());c.color(0x14c66a);c.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(w/2+89,275),28,28),c.ink.Get());c.line(w/2+75,275,w/2+86,285,0xffffff,4);c.line(w/2+86,285,w/2+105,264,0xffffff,4);}
 c.label(page==0?L"Torrent":page==1?L"Установка":L"Готово",D2D1::RectF(28,331,w-28,385),40,true,0,DWRITE_TEXT_ALIGNMENT_CENTER);
 c.label(page==0?L"Простой торрент-клиент для Windows":page==1?L"Копирование файлов…":L"Приложение успешно установлено",D2D1::RectF(22,390,w-22,430),20,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);
 if(page==0){c.button(L"Установить   →",D2D1::RectF(64,466,w-64,534),true,hover==1,22,16);c.label(L"Отмена",D2D1::RectF(64,549,w-64,585),18,false,c.muted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 if(page==1){c.box(D2D1::RectF(38,470,w-96,484),0xdce7f5,7);if(progress>0)c.gradient(D2D1::RectF(38,470,38+(w-134)*progress/100.f,484),0x168fff,0x006bfa,7);c.label(std::to_wstring(progress)+L"%",D2D1::RectF(w-82,456,w-30,496),18,false,c.muted());}
 if(page==2){c.check(66,467,launch);c.label(L"Запустить Torrent после установки",D2D1::RectF(96,456,w-24,493),18);c.button(L"Готово",D2D1::RectF(64,518,w-64,586),true,hover==1,22,16);}
 c.end();}EndPaint(window,&ps);}
static void startInstall(){if(preview){progress=72;page=1;SetTimer(window,1,40,nullptr);SetTimer(window,2,2000,nullptr);return;}
 wchar_t temp[MAX_PATH];GetTempPathW(MAX_PATH,temp);wchar_t unique[MAX_PATH];GetTempFileNameW(temp,L"Tor",0,unique);temporary=unique;progressPath=temporary;progressPath+=L".progress";
 HRSRC res=FindResourceW(instance,MAKEINTRESOURCEW(2),RT_RCDATA);if(!res){MessageBoxW(window,L"Не найден пакет установки",L"Torrent",MB_ICONERROR);return;}
 {std::ofstream f(temporary,std::ios::binary|std::ios::trunc);f.write(static_cast<char const*>(LockResource(LoadResource(instance,res))),SizeofResource(instance,res));if(!f){MessageBoxW(window,L"Не удалось подготовить установку",L"Torrent",MB_ICONERROR);return;}}
 std::wstring cmd=L"\""+temporary.wstring()+L"\" /S /PROGRESSFILE=\""+progressPath.wstring()+L"\"";STARTUPINFOW si{};si.cb=sizeof si;PROCESS_INFORMATION pi{};
 if(!CreateProcessW(temporary.c_str(),cmd.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,nullptr,&si,&pi)){std::filesystem::remove(temporary);MessageBoxW(window,L"Не удалось запустить установку",L"Torrent",MB_ICONERROR);return;}
 CloseHandle(pi.hThread);installer=pi.hProcess;page=1;SetTimer(window,1,40,nullptr);}
static void finish(){if(page==2&&launch&&!preview){PWSTR folder=nullptr;if(SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData,0,nullptr,&folder))){auto app=std::filesystem::path(folder)/L"Programs"/L"Torrent"/L"Torrent.exe";CoTaskMemFree(folder);ShellExecuteW(window,L"open",app.c_str(),nullptr,nullptr,SW_SHOWNORMAL);}}DestroyWindow(window);}
static LRESULT CALLBACK proc(HWND w,UINT m,WPARAM wp,LPARAM lp){switch(m){case WM_CREATE:window=w;visual::frame(w);return 0;case WM_NCCALCSIZE:if(wp)return 0;break;case WM_PAINT:paint();return 0;case WM_ERASEBKGND:return 1;
 case WM_NCHITTEST:{POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(w,&p);float s=GetDpiForWindow(w)/96.f;RECT r;GetClientRect(w,&r);if(p.y<60*s&&p.x<r.right-100*s)return HTCAPTION;return HTCLIENT;}
 case WM_MOUSEMOVE:{float s=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/s,y=GET_Y_LPARAM(lp)/s;int h=page==0?visual::hit(x,y,D2D1::RectF(64,466,canvas.width-64,534)):page==2?visual::hit(x,y,D2D1::RectF(64,518,canvas.width-64,586)):0;if(h!=hover){hover=h;InvalidateRect(w,nullptr,FALSE);}return 0;}
 case WM_LBUTTONUP:{float s=GetDpiForWindow(w)/96.f,x=GET_X_LPARAM(lp)/s,y=GET_Y_LPARAM(lp)/s;
 if(y<48&&x>canvas.width-48){if(page!=1)finish();}else if(y<48&&x>canvas.width-95)ShowWindow(w,SW_MINIMIZE);
 else if(page==0&&visual::hit(x,y,D2D1::RectF(64,466,canvas.width-64,534)))startInstall();else if(page==0&&y>=549&&y<585)finish();else if(page==2&&y>=456&&y<493)launch=!launch;else if(page==2&&visual::hit(x,y,D2D1::RectF(64,518,canvas.width-64,586)))finish();InvalidateRect(w,nullptr,FALSE);return 0;}
 case WM_KEYDOWN:if(wp==VK_SPACE&&page==2)launch=!launch;if(wp==VK_RETURN){if(page==0)startInstall();else if(page==2)finish();}if(wp==VK_ESCAPE&&page!=1)finish();InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_TIMER:if(wp==1&&installer){std::ifstream f(progressPath);int value;if(f>>value)progress=std::clamp(value,0,100);}if(wp==99){KillTimer(w,99);const wchar_t* paths[]={L"setup-welcome.png",L"setup-progress.png",L"setup-finish.png"};visual::capture(w,paths[page]);if(++page==3){DestroyWindow(w);return 0;}progress=72;SetTimer(w,99,250,nullptr);}if(wp==2){page=2;KillTimer(w,1);KillTimer(w,2);}if(installer&&WaitForSingleObject(installer,0)==WAIT_OBJECT_0){DWORD code;GetExitCodeProcess(installer,&code);CloseHandle(installer);installer=nullptr;std::error_code ec;std::filesystem::remove(temporary,ec);std::filesystem::remove(progressPath,ec);KillTimer(w,1);page=code?0:2;if(code)MessageBoxW(w,L"Установка не завершена. Попробуйте снова.",L"Torrent",MB_ICONERROR);}InvalidateRect(w,nullptr,FALSE);return 0;
 case WM_CLOSE:if(page!=1)finish();return 0;case WM_DESTROY:PostQuitMessage(0);return 0;}return DefWindowProcW(w,m,wp,lp);}
int WINAPI wWinMain(HINSTANCE h,HINSTANCE,PWSTR command,int show){instance=h;capturePreview=std::wstring(command)==L"--visual-test";preview=capturePreview||std::wstring(command)==L"--preview";SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);WNDCLASSW wc{};wc.hInstance=h;wc.lpfnWndProc=proc;wc.lpszClassName=L"TorrentSetupVisual";wc.hCursor=LoadCursorW(nullptr,IDC_ARROW);wc.hIcon=LoadIconW(h,MAKEINTRESOURCEW(1));RegisterClassW(&wc);float scale=GetDpiForSystem()/96.f;HWND w=CreateWindowExW(0,wc.lpszClassName,L"Torrent Setup",WS_POPUP|WS_THICKFRAME|WS_SYSMENU|WS_MINIMIZEBOX,CW_USEDEFAULT,CW_USEDEFAULT,int(520*scale),int(620*scale),nullptr,nullptr,h,nullptr);ShowWindow(w,capturePreview?SW_SHOWNOACTIVATE:show);if(capturePreview)SetTimer(w,99,250,nullptr);MSG m;while(GetMessageW(&m,nullptr,0,0)>0){TranslateMessage(&m);DispatchMessageW(&m);}CoUninitialize();return 0;}
