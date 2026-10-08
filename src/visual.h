#pragma once
#include <windows.h>
#include <dwmapi.h>
#include <d2d1.h>
#include <dwrite.h>
#include <wrl/client.h>
#include <string>
#include <map>
#include <wincodec.h>
namespace visual {
using Microsoft::WRL::ComPtr;
inline bool hit(float x,float y,D2D1_RECT_F r){return x>=r.left&&x<r.right&&y>=r.top&&y<r.bottom;}
inline void frame(HWND w,bool dark=false,bool custom=false){int corners=2;DwmSetWindowAttribute(w,33,&corners,sizeof corners);BOOL value=dark;DwmSetWindowAttribute(w,20,&value,sizeof value);COLORREF caption=dark?RGB(21,31,46):RGB(255,255,255);COLORREF border=custom?0xfffffffe:caption;DwmSetWindowAttribute(w,35,&border,sizeof border);DwmSetWindowAttribute(w,36,&caption,sizeof caption);MARGINS margins{0,0,0,0};DwmExtendFrameIntoClientArea(w,&margins);}
inline bool capture(HWND w,wchar_t const* path){RECT r;GetClientRect(w,&r);HDC dc=GetDC(w),memory=CreateCompatibleDC(dc);HBITMAP bitmap=CreateCompatibleBitmap(dc,r.right,r.bottom);auto old=SelectObject(memory,bitmap);BOOL drawn=PrintWindow(w,memory,3);SelectObject(memory,old);DeleteDC(memory);ReleaseDC(w,dc);
 ComPtr<IWICImagingFactory> f;ComPtr<IWICBitmap> source;ComPtr<IWICStream> stream;ComPtr<IWICBitmapEncoder> encoder;ComPtr<IWICBitmapFrameEncode> frame;
 HRESULT hr=CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&f));if(SUCCEEDED(hr))hr=f->CreateBitmapFromHBITMAP(bitmap,nullptr,WICBitmapIgnoreAlpha,&source);if(SUCCEEDED(hr))hr=f->CreateStream(&stream);if(SUCCEEDED(hr))hr=stream->InitializeFromFilename(path,GENERIC_WRITE);if(SUCCEEDED(hr))hr=f->CreateEncoder(GUID_ContainerFormatPng,nullptr,&encoder);if(SUCCEEDED(hr))hr=encoder->Initialize(stream.Get(),WICBitmapEncoderNoCache);if(SUCCEEDED(hr))hr=encoder->CreateNewFrame(&frame,nullptr);if(SUCCEEDED(hr))hr=frame->Initialize(nullptr);if(SUCCEEDED(hr))hr=frame->WriteSource(source.Get(),nullptr);if(SUCCEEDED(hr))hr=frame->Commit();if(SUCCEEDED(hr))hr=encoder->Commit();DeleteObject(bitmap);return drawn&&SUCCEEDED(hr);}
struct Canvas {
 ComPtr<ID2D1Factory> factory;ComPtr<IDWriteFactory> writing;ComPtr<ID2D1HwndRenderTarget> rt;ComPtr<ID2D1SolidColorBrush> ink;
 std::map<int,ComPtr<IDWriteTextFormat>> fonts;bool dark=false;float width=0,height=0,scale=1;
 UINT fg()const{return dark?0xe8eef8:0x111827;}UINT muted()const{return dark?0x9baac3:0x757d8a;}UINT card()const{return dark?0x202c3e:0xffffff;}UINT lineColor()const{return dark?0x34445b:0xe9eaed;}
 bool begin(HWND w){RECT r;GetClientRect(w,&r);scale=GetDpiForWindow(w)/96.f;width=r.right/scale;height=r.bottom/scale;
  if(!factory)D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,factory.GetAddressOf());
  if(!writing)DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(writing.GetAddressOf()));
  if(!factory||!writing)return false;
  if(!rt){factory->CreateHwndRenderTarget(D2D1::RenderTargetProperties(),D2D1::HwndRenderTargetProperties(w,D2D1::SizeU(r.right,r.bottom)),&rt);if(!rt)return false;rt->CreateSolidColorBrush(D2D1::ColorF(0),&ink);}else rt->Resize(D2D1::SizeU(r.right,r.bottom));
  rt->SetDpi(96*scale,96*scale);rt->BeginDraw();rt->Clear(D2D1::ColorF(dark?0x151f2e:0xffffff));return true;}
 void end(){if(rt->EndDraw()==D2DERR_RECREATE_TARGET){ink.Reset();rt.Reset();}}
 void color(UINT c){ink->SetColor(D2D1::ColorF(c));}
 void box(D2D1_RECT_F r,UINT c,float radius=9,UINT border=0){color(c);auto shape=D2D1::RoundedRect(r,radius,radius);rt->FillRoundedRectangle(shape,ink.Get());if(border){color(border);rt->DrawRoundedRectangle(shape,ink.Get(),1);}}
 void gradient(D2D1_RECT_F r,UINT top,UINT bottom,float radius=9){D2D1_GRADIENT_STOP stops[]={{0,D2D1::ColorF(top)},{1,D2D1::ColorF(bottom)}};ComPtr<ID2D1GradientStopCollection> collection;ComPtr<ID2D1LinearGradientBrush> gradientBrush;ComPtr<ID2D1RoundedRectangleGeometry> shape;rt->CreateGradientStopCollection(stops,2,&collection);if(!collection)return;rt->CreateLinearGradientBrush(D2D1::LinearGradientBrushProperties(D2D1::Point2F(r.left,r.top),D2D1::Point2F(r.left,r.bottom)),collection.Get(),&gradientBrush);factory->CreateRoundedRectangleGeometry(D2D1::RoundedRect(r,radius,radius),&shape);if(gradientBrush&&shape)rt->FillGeometry(shape.Get(),gradientBrush.Get());}
 void line(float x,float y,float a,float b,UINT c,float stroke=1.5f){color(c);rt->DrawLine(D2D1::Point2F(x,y),D2D1::Point2F(a,b),ink.Get(),stroke);}
 wchar_t const* fontFamily(bool display=false){static std::wstring body,heading;if(body.empty()){ComPtr<IDWriteFontCollection> collection;writing->GetSystemFontCollection(&collection);UINT32 index=0;BOOL exists=FALSE;if(collection)collection->FindFamilyName(L"Segoe UI Variable Text",&index,&exists);body=exists?L"Segoe UI Variable Text":L"Segoe UI";exists=FALSE;if(collection)collection->FindFamilyName(L"Segoe UI Variable Display",&index,&exists);heading=exists?L"Segoe UI Variable Display":body;}return display?heading.c_str():body.c_str();}
 void label(std::wstring const&s,D2D1_RECT_F r,float size=15,bool bold=false,UINT c=0,DWRITE_TEXT_ALIGNMENT align=DWRITE_TEXT_ALIGNMENT_LEADING){int key=int(size*10)+(bold?10000:0);auto& f=fonts[key];if(!f){writing->CreateTextFormat(fontFamily(size>=20),nullptr,bold?DWRITE_FONT_WEIGHT_MEDIUM:DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size,L"",&f);f->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);DWRITE_TRIMMING trim{DWRITE_TRIMMING_GRANULARITY_CHARACTER,0,0};ComPtr<IDWriteInlineObject> ellipsis;writing->CreateEllipsisTrimmingSign(f.Get(),&ellipsis);f->SetTrimming(&trim,ellipsis.Get());f->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);}f->SetTextAlignment(align);color(c?c:fg());rt->DrawText(s.c_str(),UINT32(s.size()),f.Get(),r,ink.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);}
 void button(std::wstring const&s,D2D1_RECT_F r,bool primary=false,bool hover=false,float fontSize=15,float radius=9){box(r,primary?(hover?0x0069e7:0x087cff):(hover?(dark?0x34445b:0xf1f2f4):(dark?0x263449:0xf8f8f9)),radius,primary?0:lineColor());label(s,r,fontSize,false,primary?0xffffff:fg(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 void check(float x,float y,bool checked){box(D2D1::RectF(x,y,x+16,y+16),checked?0x087cff:card(),4,checked?0:0xbdcbdc);if(checked){line(x+4,y+8,x+7,y+11,0xffffff,1.7f);line(x+7,y+11,x+12,y+5,0xffffff,1.7f);}}
 void chevron(float x,float y){line(x,y,x+4,y+4,muted());line(x+4,y+4,x+8,y,muted());}
 void close(float x,float y){line(x,y,x+10,y+10,muted());line(x+10,y,x,y+10,muted());}
 void titlebar(std::wstring const&s,bool minimize=false){label(s,D2D1::RectF(24,14,width-100,50),22,true);close(width-31,24);if(minimize)line(width-76,29,width-65,29,muted());}
 void success(float x,float y){color(0x14c66a);rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x,y),11,11),ink.Get());line(x-5,y,x-1,y+4,0xffffff,2);line(x-1,y+4,x+5,y-4,0xffffff,2);}
};
}
