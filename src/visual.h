#pragma once
#include <windows.h>
#include <dwmapi.h>
#include <d2d1.h>
#include <dwrite.h>
#include <dwrite_3.h>
#include <wrl/client.h>
#include <string>
#include <map>
#include <algorithm>
#include <wincodec.h>
namespace visual {
using Microsoft::WRL::ComPtr;
struct ProductFont {
 ComPtr<IDWriteFactory5> factory;ComPtr<IDWriteInMemoryFontFileLoader> loader;ComPtr<IDWriteFontCollection1> collection;HANDLE gdi=nullptr;
 bool load(IDWriteFactory* writing){if(collection)return true;if(FAILED(writing->QueryInterface(IID_PPV_ARGS(&factory))))return false;auto module=GetModuleHandleW(nullptr);auto resource=FindResourceW(module,MAKEINTRESOURCEW(4),RT_RCDATA);if(!resource)return false;void* bytes=LockResource(LoadResource(module,resource));DWORD length=SizeofResource(module,resource);if(!bytes||!length)return false;
  if(FAILED(factory->CreateInMemoryFontFileLoader(&loader))||FAILED(factory->RegisterFontFileLoader(loader.Get())))return false;
  ComPtr<IDWriteFontFile> file;ComPtr<IDWriteFontSetBuilder1> builder;ComPtr<IDWriteFontSet> set;
  if(FAILED(loader->CreateInMemoryFontFileReference(factory.Get(),bytes,length,nullptr,&file))||FAILED(factory->CreateFontSetBuilder(&builder))||FAILED(builder->AddFontFile(file.Get()))||FAILED(builder->CreateFontSet(&set))||FAILED(factory->CreateFontCollectionFromFontSet(set.Get(),&collection)))return false;
  UINT32 index=0;BOOL exists=FALSE;collection->FindFamilyName(L"Montserrat",&index,&exists);if(!exists){collection.Reset();return false;}ComPtr<IDWriteFontFamily> family;ComPtr<IDWriteFont> regular;static_cast<IDWriteFontCollection*>(collection.Get())->GetFontFamily(index,&family);if(!family||FAILED(family->GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STRETCH_NORMAL,DWRITE_FONT_STYLE_NORMAL,&regular))){collection.Reset();return false;}regular->HasCharacter(L'Я',&exists);if(!exists){collection.Reset();return false;}
  DWORD count=0;gdi=AddFontMemResourceEx(bytes,length,nullptr,&count);return true;
 }
 ~ProductFont(){collection.Reset();if(gdi)RemoveFontMemResourceEx(gdi);if(factory&&loader)factory->UnregisterFontFileLoader(loader.Get());}
};
inline ProductFont productFont;
namespace ui {constexpr float controlHeight=36, radius=8, body=13, label=12, help=12, title=18, header=62, progress=4, focus=2;constexpr UINT_PTR animationTimer=0xc011;}

inline bool hit(float x,float y,D2D1_RECT_F r){return x>=r.left&&x<r.right&&y>=r.top&&y<r.bottom;}
inline void frame(HWND w,bool dark=false,bool custom=false){int corners=2;DwmSetWindowAttribute(w,33,&corners,sizeof corners);BOOL value=dark;DwmSetWindowAttribute(w,20,&value,sizeof value);COLORREF caption=dark?RGB(12,12,12):RGB(245,245,244);COLORREF border=custom?0xfffffffe:caption;DwmSetWindowAttribute(w,35,&border,sizeof border);DwmSetWindowAttribute(w,36,&caption,sizeof caption);MARGINS margins{0,0,0,0};DwmExtendFrameIntoClientArea(w,&margins);}
inline bool capture(HWND w,wchar_t const* path){RECT r;GetClientRect(w,&r);HDC dc=GetDC(w),memory=CreateCompatibleDC(dc);HBITMAP bitmap=CreateCompatibleBitmap(dc,r.right,r.bottom);auto old=SelectObject(memory,bitmap);BOOL drawn=PrintWindow(w,memory,3);SelectObject(memory,old);DeleteDC(memory);ReleaseDC(w,dc);
 ComPtr<IWICImagingFactory> f;ComPtr<IWICBitmap> source;ComPtr<IWICStream> stream;ComPtr<IWICBitmapEncoder> encoder;ComPtr<IWICBitmapFrameEncode> frame;
 HRESULT hr=CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&f));if(SUCCEEDED(hr))hr=f->CreateBitmapFromHBITMAP(bitmap,nullptr,WICBitmapIgnoreAlpha,&source);if(SUCCEEDED(hr))hr=f->CreateStream(&stream);if(SUCCEEDED(hr))hr=stream->InitializeFromFilename(path,GENERIC_WRITE);if(SUCCEEDED(hr))hr=f->CreateEncoder(GUID_ContainerFormatPng,nullptr,&encoder);if(SUCCEEDED(hr))hr=encoder->Initialize(stream.Get(),WICBitmapEncoderNoCache);if(SUCCEEDED(hr))hr=encoder->CreateNewFrame(&frame,nullptr);if(SUCCEEDED(hr))hr=frame->Initialize(nullptr);if(SUCCEEDED(hr))hr=frame->WriteSource(source.Get(),nullptr);if(SUCCEEDED(hr))hr=frame->Commit();if(SUCCEEDED(hr))hr=encoder->Commit();DeleteObject(bitmap);return drawn&&SUCCEEDED(hr);}
struct Canvas {
 ComPtr<ID2D1Factory> factory;ComPtr<IDWriteFactory> writing;ComPtr<ID2D1HwndRenderTarget> rt;ComPtr<ID2D1SolidColorBrush> ink;
 ComPtr<ID2D1PathGeometry> settingsGeometry;
 std::map<int,ComPtr<IDWriteTextFormat>> fonts;bool dark=false;float width=0,height=0,scale=1;
 struct Hover {float amount=0;ULONGLONG time=0;bool hovered=false;};std::map<int,Hover> hovers;HWND host=nullptr;bool animating=false;
 bool animationMessage(HWND w,UINT m,WPARAM wp){if(m!=WM_TIMER||wp!=ui::animationTimer)return false;if(!animating)KillTimer(w,ui::animationTimer);InvalidateRect(w,nullptr,FALSE);return true;}
 UINT interactive(D2D1_RECT_F r,UINT base,UINT over,bool hovered){auto& h=hovers[int(r.top)*10000+int(r.left)];ULONGLONG now=GetTickCount64();if(h.hovered!=hovered){h.hovered=hovered;h.time=now;}float dt=h.time?std::min(1.f,float(now-h.time)/140.f):1.f;h.time=now;BOOL motion=TRUE;SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,&motion,0);float target=hovered?1.f:0.f;if(!motion)h.amount=target;else if(h.amount<target)h.amount=std::min(target,h.amount+dt);else h.amount=std::max(target,h.amount-dt);if(h.amount!=target){animating=true;SetTimer(host,ui::animationTimer,16,nullptr);}UINT result=0;for(int shift:{0,8,16})result|=UINT(((base>>shift)&255)*(1-h.amount)+((over>>shift)&255)*h.amount)<<shift;return result;}
 UINT fg()const{return dark?0xf2f2f1:0x17191c;}UINT muted()const{return dark?0x94989c:0x777c81;}UINT card()const{return dark?0x101010:0xfafafa;}UINT lineColor()const{return dark?0x303234:0xdadbd9;}UINT background()const{return dark?0x0c0c0c:0xf5f5f4;}UINT selected()const{return dark?0x222324:0xe8e9e7;}UINT track()const{return dark?0x303234:0xe0e1de;}
 bool begin(HWND w){host=w;animating=false;RECT r;GetClientRect(w,&r);scale=GetDpiForWindow(w)/96.f;width=r.right/scale;height=r.bottom/scale;
  if(!factory)D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,factory.GetAddressOf());
  if(!writing)DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(writing.GetAddressOf()));
  if(!factory||!writing)return false;
  if(!rt){factory->CreateHwndRenderTarget(D2D1::RenderTargetProperties(),D2D1::HwndRenderTargetProperties(w,D2D1::SizeU(r.right,r.bottom)),&rt);if(!rt)return false;rt->CreateSolidColorBrush(D2D1::ColorF(0),&ink);}else rt->Resize(D2D1::SizeU(r.right,r.bottom));
  productFont.load(writing.Get());rt->SetDpi(96*scale,96*scale);rt->BeginDraw();rt->Clear(D2D1::ColorF(background()));return true;}
 void end(){if(rt->EndDraw()==D2DERR_RECREATE_TARGET){ink.Reset();rt.Reset();}}
 void color(UINT c){ink->SetColor(D2D1::ColorF(c));}
 void box(D2D1_RECT_F r,UINT c,float radius=ui::radius,UINT border=0){color(c);auto shape=D2D1::RoundedRect(r,radius,radius);rt->FillRoundedRectangle(shape,ink.Get());if(border){color(border);rt->DrawRoundedRectangle(shape,ink.Get(),1);}}
 void gradient(D2D1_RECT_F r,UINT top,UINT bottom,float radius=9){D2D1_GRADIENT_STOP stops[]={{0,D2D1::ColorF(top)},{1,D2D1::ColorF(bottom)}};ComPtr<ID2D1GradientStopCollection> collection;ComPtr<ID2D1LinearGradientBrush> gradientBrush;ComPtr<ID2D1RoundedRectangleGeometry> shape;rt->CreateGradientStopCollection(stops,2,&collection);if(!collection)return;rt->CreateLinearGradientBrush(D2D1::LinearGradientBrushProperties(D2D1::Point2F(r.left,r.top),D2D1::Point2F(r.left,r.bottom)),collection.Get(),&gradientBrush);factory->CreateRoundedRectangleGeometry(D2D1::RoundedRect(r,radius,radius),&shape);if(gradientBrush&&shape)rt->FillGeometry(shape.Get(),gradientBrush.Get());}
 void line(float x,float y,float a,float b,UINT c,float stroke=1.5f){color(c);rt->DrawLine(D2D1::Point2F(x,y),D2D1::Point2F(a,b),ink.Get(),stroke);}
 wchar_t const* fontFamily(bool=false){return productFont.collection?L"Montserrat":L"Segoe UI";}
 void label(std::wstring const&s,D2D1_RECT_F r,float size=15,bool bold=false,UINT c=0,DWRITE_TEXT_ALIGNMENT align=DWRITE_TEXT_ALIGNMENT_LEADING,float tracking=0){int key=int(size*10)+(bold?10000:0);auto& f=fonts[key];if(!f){writing->CreateTextFormat(fontFamily(),productFont.collection.Get(),bold?DWRITE_FONT_WEIGHT_MEDIUM:DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size,L"",&f);if(!f)return;f->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);DWRITE_TRIMMING trim{DWRITE_TRIMMING_GRANULARITY_CHARACTER,0,0};ComPtr<IDWriteInlineObject> ellipsis;writing->CreateEllipsisTrimmingSign(f.Get(),&ellipsis);f->SetTrimming(&trim,ellipsis.Get());f->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);}f->SetTextAlignment(align);color(c?c:fg());if(tracking){ComPtr<IDWriteTextLayout> layout;writing->CreateTextLayout(s.c_str(),UINT32(s.size()),f.Get(),r.right-r.left,r.bottom-r.top,&layout);ComPtr<IDWriteTextLayout1> spaced;if(layout&&SUCCEEDED(layout.As(&spaced))){spaced->SetCharacterSpacing(tracking/2,tracking/2,0,{0,UINT32(s.size())});rt->DrawTextLayout(D2D1::Point2F(r.left,r.top),layout.Get(),ink.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);}}else rt->DrawText(s.c_str(),UINT32(s.size()),f.Get(),r,ink.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);}
 void button(std::wstring const&s,D2D1_RECT_F r,bool primary=false,bool hover=false,float fontSize=ui::body,float radius=ui::radius,bool enabled=true){UINT fill=interactive(r,primary?fg():card(),primary?(dark?0xdadbd9:0x303234):selected(),enabled&&hover);if(enabled&&hover&&(GetKeyState(VK_LBUTTON)&0x8000))fill=primary?(dark?0xdadbd9:0x303234):selected();box(r,enabled?fill:card(),radius,primary&&enabled?0:lineColor());label(s,r,fontSize,false,!enabled?muted():primary?background():fg(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 void focus(D2D1_RECT_F r){color(muted());rt->DrawRoundedRectangle(D2D1::RoundedRect(r,ui::radius,ui::radius),ink.Get(),ui::focus);}
 void settingsIcon(float x,float y,float size,bool hover=false){if(!settingsGeometry){ComPtr<ID2D1GeometrySink> sink;if(FAILED(factory->CreatePathGeometry(&settingsGeometry))||FAILED(settingsGeometry->Open(&sink)))return;sink->SetFillMode(D2D1_FILL_MODE_ALTERNATE);
#include "../assets/settings08.inc"
 if(FAILED(sink->Close())){settingsGeometry.Reset();return;}}
 D2D1_MATRIX_3X2_F previous;rt->GetTransform(&previous);float scale=size/85.f;rt->SetTransform(D2D1::Matrix3x2F::Scale(scale,scale)*D2D1::Matrix3x2F::Translation(x-5*scale,y)*previous);color(hover?fg():muted());rt->FillGeometry(settingsGeometry.Get(),ink.Get());rt->SetTransform(previous);}
 void switchControl(float x,float y,bool on){box(D2D1::RectF(x,y,x+40,y+23),on?fg():card(),11.5f,lineColor());color(on?background():muted());rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x+(on?28.5f:11.5f),y+11.5f),7.5f,7.5f),ink.Get());}
 void slider(float x,float y,float length,float value){line(x,y,x+length,y,track(),2);line(x,y,x+length*value,y,fg(),2);color(fg());rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x+length*value,y),6,6),ink.Get());}
 void paragraph(std::wstring const& text,D2D1_RECT_F r){ComPtr<IDWriteTextFormat> f;writing->CreateTextFormat(fontFamily(),productFont.collection.Get(),DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,ui::body,L"",&f);if(f){color(fg());rt->DrawText(text.c_str(),UINT32(text.size()),f.Get(),r,ink.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);}}
 void check(float x,float y,bool checked){box(D2D1::RectF(x,y,x+16,y+16),checked?fg():card(),4,checked?0:lineColor());if(checked){line(x+4,y+8,x+7,y+11,background(),1.7f);line(x+7,y+11,x+12,y+5,background(),1.7f);}}
 void chevron(float x,float y){line(x,y,x+4,y+4,muted());line(x+4,y+4,x+8,y,muted());}
 void close(float x,float y){line(x,y,x+10,y+10,muted());line(x+10,y,x,y+10,muted());}
 void titlebar(std::wstring const&s,bool minimize=false){auto title=s;CharUpperBuffW(title.data(),DWORD(title.size()));label(title,D2D1::RectF(24,14,width-65,50),ui::title,false,0,DWRITE_TEXT_ALIGNMENT_LEADING,1.6f);close(width-31,24);if(minimize)line(width-76,29,width-65,29,muted());}
 void success(float x,float y){color(fg());rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x,y),11,11),ink.Get());line(x-5,y,x-1,y+4,background(),2);line(x-1,y+4,x+5,y-4,background(),2);}
};
}
