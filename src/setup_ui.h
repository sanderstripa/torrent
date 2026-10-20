#pragma once
#include <cmath>
// CONNECT installer geometry and tokens, local to setup; shared application visuals stay unchanged.
static bool setupEnglish=false,desktopShortcut=true,startMenuShortcut=true;
static UINT setupBg(){return canvas.dark?0x202122:0xf2f0eb;}
static UINT setupFg(){return canvas.dark?0xedede9:0x202624;}
static UINT setupMuted(){return canvas.dark?0xa3a6a5:0x687071;}
static UINT setupLine(){return canvas.dark?0x414344:0xd9d8d2;}
static UINT setupButton(){return canvas.dark?0xe3e4df:0x242a28;}
static UINT setupInverse(){return canvas.dark?0x202122:0xffffff;}
static UINT setupField(){return canvas.dark?0x292b2c:0xebe9e4;}
static std::wstring st(wchar_t const* ru,wchar_t const* en){return setupEnglish?en:ru;}
#include "setup_geometry.h"
static bool setupAxisValid=true;
// Match CSS font-weight using the real Montserrat variable wght axis, never a synthetic weight.
static void setupFontWeight(IDWriteTextFormat* format,IDWriteTextLayout* layout,DWRITE_FONT_WEIGHT weight,UINT32 count){
 DWRITE_FONT_AXIS_VALUE axis{DWRITE_FONT_AXIS_TAG_WEIGHT,float(weight)};
 if(format){ComPtr<IDWriteTextFormat3> variable;if(FAILED(format->QueryInterface(IID_PPV_ARGS(&variable)))||FAILED(variable->SetFontAxisValues(&axis,1)))setupAxisValid=false;}
 if(layout){ComPtr<IDWriteTextLayout4> variable;if(FAILED(layout->QueryInterface(IID_PPV_ARGS(&variable)))||FAILED(variable->SetFontAxisValues(&axis,1,{0,count})))setupAxisValid=false;}
}
// Chromium rounds the source font's hhea ascent/descent to CSS pixels before line layout.
// Montserrat hhea: ascent 968, descent 251, unitsPerEm 1000; measured in CONNECT at 100%.
static void setupLineMetrics(IDWriteTextFormat* format,float size){float ascent=std::round(size*.968f),descent=std::round(size*.251f);format->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM,ascent+descent,ascent);}
// Use one DirectWrite renderer for every setup label and the editable path.
static void setupText(visual::Canvas& c,std::wstring const& value,D2D1_RECT_F r,float size,UINT tint,DWRITE_TEXT_ALIGNMENT align=DWRITE_TEXT_ALIGNMENT_LEADING,DWRITE_FONT_WEIGHT weight=DWRITE_FONT_WEIGHT_NORMAL,float tracking=0){
 ComPtr<IDWriteTextFormat> format;ComPtr<IDWriteTextLayout> layout;
 c.writing->CreateTextFormat(c.fontFamily(),visual::productFont.collection.Get(),weight,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size,L"",&format);if(!format)return;
 setupFontWeight(format.Get(),nullptr,weight,0);setupLineMetrics(format.Get(),size);format->SetTextAlignment(align);format->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
 c.writing->CreateTextLayout(value.c_str(),UINT32(value.size()),format.Get(),r.right-r.left,r.bottom-r.top,&layout);if(!layout)return;setupFontWeight(nullptr,layout.Get(),weight,UINT32(value.size()));
 if(tracking){ComPtr<IDWriteTextLayout1> spaced;if(SUCCEEDED(layout.As(&spaced)))spaced->SetCharacterSpacing(tracking/2,tracking/2,0,{0,UINT32(value.size())});}
 c.color(tint);c.rt->DrawTextLayout(D2D1::Point2F(r.left,r.top),layout.Get(),c.ink.Get(),D2D1_DRAW_TEXT_OPTIONS(D2D1_DRAW_TEXT_OPTIONS_CLIP|D2D1_DRAW_TEXT_OPTIONS_NO_SNAP));
}
static void text(std::wstring const& s,D2D1_RECT_F r,float size=13,UINT color=0,DWRITE_TEXT_ALIGNMENT align=DWRITE_TEXT_ALIGNMENT_LEADING,bool bold=false,float tracking=0){setupText(canvas,s,r,size,color?color:setupFg(),align,bold?DWRITE_FONT_WEIGHT_SEMI_BOLD:DWRITE_FONT_WEIGHT_NORMAL,tracking);}
static void primary(std::wstring const& s,D2D1_RECT_F r){canvas.box(r,setupButton(),10);setupText(canvas,s,r,14,setupInverse(),DWRITE_TEXT_ALIGNMENT_CENTER,DWRITE_FONT_WEIGHT_SEMI_BOLD,.2f);}
static void setupSwitch(float x,float y,bool checked){canvas.box(D2D1::RectF(x,y,x+38,y+22),checked?setupButton():setupBg(),11,checked?0:setupMuted());canvas.color(checked?setupInverse():setupMuted());canvas.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x+(checked?27:11),y+11),7,7),canvas.ink.Get());}
static std::wstring installDirectory;
static std::wstring setupPath(){if(!installDirectory.empty())return installDirectory;PWSTR folder=nullptr;std::filesystem::path path;if(SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData,0,nullptr,&folder))){path=std::filesystem::path(folder)/L"Programs"/L"Torrent";CoTaskMemFree(folder);}return path.wstring();}
static D2D1_RECT_F primaryRect(){return page==0?D2D1::RectF(44,287,272,336):page==3?settingsGeometry().accept:D2D1::RectF(306,312.25f,534,361.25f);}
static void drawRibbon(D2D1_RECT_F r){if(!icon)loadIcon();if(icon){auto size=icon->GetSize();float ratio=std::min((r.right-r.left)/size.width,(r.bottom-r.top)/size.height);float width=size.width*ratio,height=size.height*ratio;float x=(r.left+r.right-width)/2,y=(r.top+r.bottom-height)/2;canvas.rt->DrawBitmap(icon.Get(),D2D1::RectF(x,y,x+width,y+height));}}
static void setupBox(D2D1_RECT_F r,UINT fill,float radius,UINT border){canvas.box(r,fill,radius);if(border){canvas.color(border);canvas.rt->DrawRoundedRectangle(D2D1::RoundedRect(D2D1::RectF(r.left+.5f,r.top+.5f,r.right-.5f,r.bottom-.5f),radius-.5f,radius-.5f),canvas.ink.Get(),1);}}
static void choice(std::wstring const& s,D2D1_RECT_F r,bool selected){if(selected)setupBox(r,setupField(),5,setupMuted());text(s,r,11,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER);}
static void paint(){PAINTSTRUCT ps;BeginPaint(window,&ps);if(canvas.begin(window)){auto& c=canvas;float w=c.width;c.rt->Clear(D2D1::ColorF(setupBg()));c.rt->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
 c.line(w-105,36,w-95,36,setupMuted(),1);c.line(w-62,32,w-54,40,setupMuted(),1);c.line(w-54,32,w-62,40,setupMuted(),1);
 c.line(44,490,w-44,490,setupLine(),1);text(L"by CONNECT.",D2D1::RectF(44,500,270,532),10,setupMuted(),DWRITE_TEXT_ALIGNMENT_LEADING,false,1);text(st(L"Версия 0.3.1",L"Version 0.3.1"),D2D1::RectF(w-220,500,w-44,532),10,setupMuted(),DWRITE_TEXT_ALIGNMENT_TRAILING,false,1);
 if(page==0){drawRibbon(D2D1::RectF(377,61,817,426));text(st(L"УСТАНОВКА",L"INSTALLATION"),D2D1::RectF(44,165,380,193),12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true,4);setupText(c,L"Torrent",D2D1::RectF(44,202,410,258),42,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,DWRITE_FONT_WEIGHT_MEDIUM,-1.8f);primary(st(L"Установить",L"Install"),primaryRect());text(st(L"Настройки установки",L"Installation settings"),D2D1::RectF(44,350,330,382),13,setupMuted());}
 if(page==1){drawRibbon(D2D1::RectF(185,69,655,374));c.box(D2D1::RectF(44,435,w-44,439),setupLine(),2);if(progress>0)c.box(D2D1::RectF(44,435,44+(w-88)*progress/100.f,439),setupFg(),2);text(st(L"Установка…",L"Installing…"),D2D1::RectF(44,443,w-44,475),11,setupMuted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 if(page==2){setupText(c,L"Torrent",D2D1::RectF(44,131.75f,w-44,183.75f),42,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER,DWRITE_FONT_WEIGHT_MEDIUM,-1.8f);text(st(L"Установка завершена",L"Installation complete"),D2D1::RectF(44,195.75f,w-44,213.75f),14,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER);primary(st(L"Готово",L"Done"),primaryRect());float start=setupEnglish?289:279;setupSwitch(start,384.25f,launch);text(st(L"Запустить Torrent после установки",L"Launch Torrent after installation"),D2D1::RectF(start+51,388.25f,w-120,402.25f),11);}
 if(page==3){auto g=settingsGeometry();
 setupText(c,st(L"Настройки установки",L"Installation settings"),g.settingsTitle,23,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,DWRITE_FONT_WEIGHT_MEDIUM);text(st(L"Назад",L"Back"),g.back,13,setupMuted());
 text(st(L"Язык",L"Language"),g.languageLabel,12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true);choice(L"RU",g.ru,!setupEnglish);choice(L"EN",g.en,setupEnglish);
 text(st(L"Тема",L"Theme"),g.themeLabel,12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true);choice(st(L"Светлая",L"Light"),g.light,!c.dark);choice(st(L"Тёмная",L"Dark"),g.dark,c.dark);
 text(st(L"Папка установки",L"Installation folder"),g.pathLabel,12,setupFg());setupBox(g.path,setupField(),6,setupLine());setupBox(g.browse,setupBg(),6,setupLine());text(st(L"Выбрать…",L"Browse…"),g.browse,12,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER);
 setupSwitch(g.desktopRow.left,g.desktopRow.top+8,desktopShortcut);text(st(L"Ярлык на рабочем столе",L"Desktop shortcut"),g.desktopLabel,13);
 setupSwitch(g.menuRow.left,g.menuRow.top+8,startMenuShortcut);text(st(L"Ярлык в меню «Пуск»",L"Start menu shortcut"),g.menuLabel,13);primary(st(L"Принять и установить",L"Accept and install"),g.accept);}
 c.end();if(!canvas.rt)icon.Reset();}EndPaint(window,&ps);}
