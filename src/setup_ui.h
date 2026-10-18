#pragma once
// CONNECT installer geometry and tokens, local to setup; shared application visuals stay unchanged.
static bool setupEnglish=false;
static UINT setupBg(){return canvas.dark?0x202122:0xf2f0eb;}
static UINT setupFg(){return canvas.dark?0xedede9:0x202624;}
static UINT setupMuted(){return canvas.dark?0xa3a6a5:0x687071;}
static UINT setupLine(){return canvas.dark?0x414344:0xd9d8d2;}
static UINT setupButton(){return canvas.dark?0xe3e4df:0x242a28;}
static UINT setupInverse(){return canvas.dark?0x202122:0xffffff;}
static UINT setupField(){return canvas.dark?0x292b2c:0xebe9e4;}
static std::wstring st(wchar_t const* ru,wchar_t const* en){return setupEnglish?en:ru;}
static void text(std::wstring const& s,D2D1_RECT_F r,float size=13,UINT color=0,DWRITE_TEXT_ALIGNMENT align=DWRITE_TEXT_ALIGNMENT_LEADING,bool bold=false,float tracking=0){canvas.label(s,r,size,bold,color?color:setupFg(),align,tracking);}
static void primary(std::wstring const& s,D2D1_RECT_F r){canvas.box(r,setupButton(),10);text(s,r,14,setupInverse(),DWRITE_TEXT_ALIGNMENT_CENTER,true,.2f);}
static void switchLaunch(float x,float y){canvas.box(D2D1::RectF(x,y,x+38,y+22),launch?setupButton():setupBg(),11,launch?0:setupMuted());canvas.color(launch?setupInverse():setupMuted());canvas.rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(x+(launch?27:11),y+11),7,7),canvas.ink.Get());}
static std::wstring setupPath(){PWSTR folder=nullptr;std::filesystem::path path;if(SUCCEEDED(SHGetKnownFolderPath(FOLDERID_LocalAppData,0,nullptr,&folder))){path=std::filesystem::path(folder)/L"Programs"/L"Torrent";CoTaskMemFree(folder);}return path.wstring();}
static D2D1_RECT_F primaryRect(){return page==0?D2D1::RectF(44,287,272,336):page==3?D2D1::RectF(44,327,304,376):D2D1::RectF(306,286,534,335);}
static void drawRibbon(D2D1_RECT_F r){if(!icon)loadIcon();if(icon){auto size=icon->GetSize();float ratio=std::min((r.right-r.left)/size.width,(r.bottom-r.top)/size.height);float width=size.width*ratio,height=size.height*ratio;float x=(r.left+r.right-width)/2,y=(r.top+r.bottom-height)/2;canvas.rt->DrawBitmap(icon.Get(),D2D1::RectF(x,y,x+width,y+height));}}
static void choice(std::wstring const& s,D2D1_RECT_F r,bool selected){if(selected)canvas.box(r,setupField(),5,setupMuted());text(s,r,11,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER);}
static void paint(){PAINTSTRUCT ps;BeginPaint(window,&ps);if(canvas.begin(window)){auto& c=canvas;float w=c.width;c.rt->Clear(D2D1::ColorF(setupBg()));
 c.line(w-105,36,w-95,36,setupMuted(),1);c.line(w-62,32,w-54,40,setupMuted(),1);c.line(w-54,32,w-62,40,setupMuted(),1);
 c.line(44,490,w-44,490,setupLine(),1);text(L"by CONNECT.",D2D1::RectF(44,500,270,532),10,setupMuted(),DWRITE_TEXT_ALIGNMENT_LEADING,false,1);text(st(L"Версия 0.2.4",L"Version 0.2.4"),D2D1::RectF(w-220,500,w-44,532),10,setupMuted(),DWRITE_TEXT_ALIGNMENT_TRAILING,false,1);
 if(page==0){drawRibbon(D2D1::RectF(410,65,818,389));text(st(L"УСТАНОВКА",L"INSTALLATION"),D2D1::RectF(44,165,380,193),12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true,4);text(L"Torrent",D2D1::RectF(44,202,410,258),42,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,false,-1.8f);primary(st(L"Установить",L"Install"),primaryRect());text(st(L"Настройки установки",L"Installation settings"),D2D1::RectF(44,350,330,382),13,setupMuted());}
 if(page==1){drawRibbon(D2D1::RectF(204,80,636,370));c.box(D2D1::RectF(44,435,w-44,439),setupLine(),2);if(progress>0)c.box(D2D1::RectF(44,435,44+(w-88)*progress/100.f,439),setupFg(),2);text(st(L"Установка…",L"Installing…"),D2D1::RectF(44,443,w-44,475),11,setupMuted(),DWRITE_TEXT_ALIGNMENT_CENTER);}
 if(page==2){text(L"Torrent",D2D1::RectF(44,150,w-44,209),42,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER,false,-1.8f);text(st(L"Установка завершена",L"Installation complete"),D2D1::RectF(44,218,w-44,249),14,setupFg(),DWRITE_TEXT_ALIGNMENT_CENTER);primary(st(L"Готово",L"Done"),primaryRect());float start=setupEnglish?289:279;switchLaunch(start,358);text(st(L"Запустить Torrent после установки",L"Launch Torrent after installation"),D2D1::RectF(start+51,352,w-120,387),11);}
 if(page==3){text(st(L"Настройки установки",L"Installation settings"),D2D1::RectF(44,60,650,101),23);text(st(L"Назад",L"Back"),D2D1::RectF(w-110,60,w-44,101),13,setupMuted(),DWRITE_TEXT_ALIGNMENT_TRAILING);
 text(st(L"Язык",L"Language"),D2D1::RectF(44,116,92,143),12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true);choice(L"RU",D2D1::RectF(93,116,129,144),!setupEnglish);choice(L"EN",D2D1::RectF(133,116,169,144),setupEnglish);text(st(L"Тема",L"Theme"),D2D1::RectF(249,116,291,143),12,setupFg(),DWRITE_TEXT_ALIGNMENT_LEADING,true);choice(st(L"Светлая",L"Light"),D2D1::RectF(296,116,362,144),!c.dark);choice(st(L"Тёмная",L"Dark"),D2D1::RectF(366,116,432,144),c.dark);
 text(st(L"Папка установки",L"Installation folder"),D2D1::RectF(44,157,w-44,182),12,setupMuted());c.box(D2D1::RectF(44,185,w-44,220),setupField(),6,setupLine());text(setupPath(),D2D1::RectF(56,185,w-56,220),12);text(st(L"Ярлык в меню «Пуск» создаётся автоматически",L"A Start menu shortcut is created automatically"),D2D1::RectF(44,236,w-44,267),13);primary(st(L"Принять и установить",L"Accept and install"),primaryRect());}
 c.end();if(!canvas.rt)icon.Reset();}EndPaint(window,&ps);}
