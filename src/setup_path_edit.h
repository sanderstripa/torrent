#pragma once
// Native EDIT retains text/IME/clipboard behavior; DirectWrite owns its visible presentation.
static WNDPROC originalPathProc=nullptr;
static visual::Canvas pathCanvas;
static float pathScroll=0;
static bool pathCaret=true;
static constexpr UINT_PTR pathCaretTimer=0xc025;
static void paintPath(HWND w){PAINTSTRUCT ps;BeginPaint(w,&ps);auto& c=pathCanvas;
 if(c.begin(w)){c.rt->Clear(D2D1::ColorF(setupField()));c.rt->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
  int length=GetWindowTextLengthW(w);std::wstring value(length+1,L'\0');GetWindowTextW(w,value.data(),length+1);value.resize(length);
  ComPtr<IDWriteTextFormat> format;ComPtr<IDWriteTextLayout> layout;c.writing->CreateTextFormat(c.fontFamily(),visual::productFont.collection.Get(),DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,12,L"",&format);
  if(format){setupFontWeight(format.Get(),nullptr,DWRITE_FONT_WEIGHT_NORMAL,0);format->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);format->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);c.writing->CreateTextLayout(value.c_str(),UINT32(value.size()),format.Get(),1000000,c.height,&layout);}
  if(layout){setupFontWeight(nullptr,layout.Get(),DWRITE_FONT_WEIGHT_NORMAL,UINT32(value.size()));DWORD a=0,b=0;SendMessageW(w,EM_GETSEL,reinterpret_cast<WPARAM>(&a),reinterpret_cast<LPARAM>(&b));float caretX=0,caretY=0;DWRITE_HIT_TEST_METRICS caret{};layout->HitTestTextPosition(b,FALSE,&caretX,&caretY,&caret);
   if(GetFocus()==w){if(caretX-pathScroll>c.width-2)pathScroll=caretX-c.width+2;if(caretX<pathScroll)pathScroll=caretX;}
   if(a!=b&&GetFocus()==w){UINT32 count=0;layout->HitTestTextRange(std::min(a,b),std::max(a,b)-std::min(a,b),-pathScroll,0,nullptr,0,&count);std::vector<DWRITE_HIT_TEST_METRICS> spans(count);if(count&&SUCCEEDED(layout->HitTestTextRange(std::min(a,b),std::max(a,b)-std::min(a,b),-pathScroll,0,spans.data(),count,&count))){for(auto const& span:spans)c.box(D2D1::RectF(span.left,span.top,span.left+span.width,span.top+span.height),canvas.dark?0x414344:0xd9d8d2,0);}}
   c.color(setupFg());c.rt->DrawTextLayout(D2D1::Point2F(-pathScroll,0),layout.Get(),c.ink.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);
   if(GetFocus()==w&&a==b&&pathCaret)c.line(caretX-pathScroll,2,caretX-pathScroll,c.height-2,setupFg(),1);
  }
  c.end();
 }EndPaint(w,&ps);HideCaret(w);
}
static LRESULT CALLBACK pathProc(HWND w,UINT m,WPARAM wp,LPARAM lp){
 if(m==WM_SETTEXT)pathScroll=0;
 if(m==WM_ERASEBKGND)return 1;
 if(m==WM_PAINT){paintPath(w);return 0;}
 if(m==WM_TIMER&&wp==pathCaretTimer){pathCaret=!pathCaret;InvalidateRect(w,nullptr,FALSE);return 0;}
 if(m==WM_KEYDOWN&&wp=='A'&&(GetKeyState(VK_CONTROL)&0x8000)){SendMessageW(w,EM_SETSEL,0,-1);InvalidateRect(w,nullptr,FALSE);return 0;}
 auto result=CallWindowProcW(originalPathProc,w,m,wp,lp);
 if(m==WM_SETFOCUS){HideCaret(w);pathCaret=true;SetTimer(w,pathCaretTimer,GetCaretBlinkTime(),nullptr);InvalidateRect(w,nullptr,FALSE);}
 if(m==WM_KILLFOCUS){KillTimer(w,pathCaretTimer);InvalidateRect(w,nullptr,FALSE);}
 if(m==WM_CHAR||m==WM_KEYDOWN||m==WM_SETTEXT||m==WM_LBUTTONDOWN||m==WM_LBUTTONUP||m==WM_MOUSEMOVE||m==WM_PASTE||m==WM_CUT||m==EM_SETSEL){HideCaret(w);pathCaret=true;InvalidateRect(w,nullptr,FALSE);}
 return result;
}
