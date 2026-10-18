#pragma once
#include <windows.h>
#include <filesystem>
#include <fstream>
#include <algorithm>

// Position only: keep the approved default dimensions and never persist a minimized rectangle.
class WindowPosition {
 std::filesystem::path file;
 RECT normal{},savedWork{},written{};
 bool enabled=false,ready=false,hasNormal=false,hasWritten=false;
 static bool reasonable(LONG value){return value>=-1000000&&value<=1000000;}
 static MONITORINFO monitor(RECT const& rectangle){MONITORINFO info{sizeof info};GetMonitorInfoW(MonitorFromRect(&rectangle,MONITOR_DEFAULTTONEAREST),&info);return info;}
public:
 static constexpr UINT_PTR timer=0xc012;
 void restore(HWND w,std::filesystem::path const& root,bool active) noexcept {
  enabled=active;file=root/L"window-position.txt";if(!enabled)return;
  try {
   int version=0;LONG x=0,y=0;RECT work{};
   {std::ifstream input(file);if(!(input>>version>>x>>y>>work.left>>work.top>>work.right>>work.bottom)||version!=1)return;}
   if(!reasonable(x)||!reasonable(y)||!reasonable(work.left)||!reasonable(work.top)||!reasonable(work.right)||!reasonable(work.bottom)||work.right<=work.left||work.bottom<=work.top)return;
   RECT current;GetWindowRect(w,&current);LONG width=current.right-current.left,height=current.bottom-current.top;RECT candidate{x,y,x+width,y+height};auto info=monitor(candidate);if(info.rcWork.right<=info.rcWork.left||info.rcWork.bottom<=info.rcWork.top)return;
   // Preserve intentional edge/monitor-straddling placement while the work area is unchanged.
   RECT visible{};bool same=EqualRect(&work,&info.rcWork)!=0;
   RECT title{x,y,x+width,y+std::min<LONG>(height,62)};
   bool reachable=IntersectRect(&visible,&title,&info.rcWork)&&(visible.right-visible.left>=std::min<LONG>(width,100))&&(visible.bottom-visible.top>=24);
   if(!same||!reachable){x=std::clamp(x,info.rcWork.left,std::max(info.rcWork.left,info.rcWork.right-width));y=std::clamp(y,info.rcWork.top,std::max(info.rcWork.top,info.rcWork.bottom-height));}
   SetWindowPos(w,nullptr,x,y,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE);
  }catch(...){/* A missing or damaged position file must not prevent startup. */}
 }
 void start(HWND w) noexcept {ready=true;capture(w);}
 void capture(HWND w) noexcept {
  if(!enabled||!ready||!IsWindowVisible(w)||IsIconic(w)||IsZoomed(w))return;
  RECT r{};if(!GetWindowRect(w,&r)||r.right<=r.left||r.bottom<=r.top)return;
  normal=r;hasNormal=true;
 }
 void moved(HWND w) noexcept {capture(w);if(enabled&&ready&&hasNormal&&!IsIconic(w))SetTimer(w,timer,250,nullptr);}
 void save(HWND w) noexcept {
  KillTimer(w,timer);if(!enabled||!ready)return;capture(w);if(!hasNormal)return;
  auto info=monitor(normal);if(hasWritten&&written.left==normal.left&&written.top==normal.top&&EqualRect(&savedWork,&info.rcWork))return;
  try {
   auto temporary=file;temporary+=L".tmp";
   {std::ofstream output(temporary,std::ios::trunc);output<<1<<' '<<normal.left<<' '<<normal.top<<' '<<info.rcWork.left<<' '<<info.rcWork.top<<' '<<info.rcWork.right<<' '<<info.rcWork.bottom<<'\n';output.flush();if(!output)return;}
   if(MoveFileExW(temporary.c_str(),file.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)){written=normal;savedWork=info.rcWork;hasWritten=true;}
  }catch(...){/* Position persistence is best effort; download/session saving is independent. */}
 }
};
