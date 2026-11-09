#pragma once
#include "visual.h"
namespace visual {
struct SettingsBackdrop {
 std::vector<unsigned char> pixels;UINT width=0,height=0;ComPtr<ID2D1Bitmap> bitmap;ID2D1RenderTarget* owner=nullptr;
 void reset(){bitmap.Reset();owner=nullptr;}
 bool capture(HWND window,bool dark){reset();RECT r;GetClientRect(window,&r);width=r.right;height=r.bottom;if(!width||!height)return false;
  BITMAPINFO info{};info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);info.bmiHeader.biWidth=width;info.bmiHeader.biHeight=-LONG(height);info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
  HDC dc=GetDC(window),memory=CreateCompatibleDC(dc);void* data=nullptr;HBITMAP dib=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&data,nullptr,0);if(!dib){DeleteDC(memory);ReleaseDC(window,dc);return false;}auto old=SelectObject(memory,dib);BOOL ok=BitBlt(memory,0,0,width,height,dc,0,0,SRCCOPY);if(ok)pixels.assign(static_cast<unsigned char*>(data),static_cast<unsigned char*>(data)+size_t(width)*height*4);SelectObject(memory,old);DeleteObject(dib);DeleteDC(memory);ReleaseDC(window,dc);if(!ok){pixels.clear();return false;}
  int radius=std::max(3,int(7*GetDpiForWindow(window)/96.f));std::vector<unsigned char> temp(pixels.size());
  for(int pass=0;pass<3;++pass)for(int axis=0;axis<2;++axis){int lines=axis?int(width):int(height),length=axis?int(height):int(width);for(int line=0;line<lines;++line)for(int channel=0;channel<3;++channel){auto at=[&](int n){int x=axis?line:n,y=axis?n:line;return (size_t(y)*width+x)*4+channel;};int sum=0;for(int k=-radius;k<=radius;++k)sum+=pixels[at(std::clamp(k,0,length-1))];for(int n=0;n<length;++n){temp[at(n)]=static_cast<unsigned char>(sum/(radius*2+1));sum-=pixels[at(std::clamp(n-radius,0,length-1))];sum+=pixels[at(std::clamp(n+radius+1,0,length-1))];}}pixels.swap(temp);}
  for(size_t i=0;i<pixels.size();i+=4){for(int c=0;c<3;++c)pixels[i+c]=static_cast<unsigned char>(pixels[i+c]*(dark?.55f:.48f)+(dark?13.f:248.f)*(dark?.45f:.52f));pixels[i+3]=255;}return true;
 }
 void draw(Canvas& c){if(pixels.empty()){c.box(D2D1::RectF(0,0,c.width,c.height),c.dark?0x303136:0xe8e9ed,0);return;}if(owner!=c.rt.Get()){reset();owner=c.rt.Get();}if(!bitmap)c.rt->CreateBitmap(D2D1::SizeU(width,height),pixels.data(),width*4,D2D1::BitmapProperties(D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_IGNORE)),&bitmap);if(bitmap)c.rt->DrawBitmap(bitmap.Get(),D2D1::RectF(0,0,c.width,c.height));}
};
}
