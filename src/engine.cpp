#include "engine.h"
#include <windows.h>
#include <libtorrent/alert_types.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/write_resume_data.hpp>
#include <libtorrent/settings_pack.hpp>
#include <fstream>
#include <iterator>
#include <chrono>
#include <sstream>
#include <algorithm>
std::string utf8(std::wstring const& v) { if(v.empty()) return {}; int n=WideCharToMultiByte(CP_UTF8,0,v.data(),int(v.size()),nullptr,0,nullptr,nullptr); std::string s(n,0); WideCharToMultiByte(CP_UTF8,0,v.data(),int(v.size()),s.data(),n,nullptr,nullptr); return s; }
std::wstring wide(std::string const& v) { if(v.empty()) return {}; int n=MultiByteToWideChar(CP_UTF8,0,v.data(),int(v.size()),nullptr,0); std::wstring s(n,0); MultiByteToWideChar(CP_UTF8,0,v.data(),int(v.size()),s.data(),n); return s; }
static lt::settings_pack settings() { lt::settings_pack p; p.set_int(lt::settings_pack::active_downloads,2); p.set_int(lt::settings_pack::active_seeds,1); p.set_int(lt::settings_pack::active_limit,3); p.set_int(lt::settings_pack::connections_limit,80); p.set_int(lt::settings_pack::alert_mask,static_cast<int>(static_cast<std::uint32_t>(lt::alert_category::error|lt::alert_category::storage|lt::alert_category::status))); p.set_str(lt::settings_pack::user_agent,"Torrent/0.1"); return p; }
static std::string key(lt::torrent_handle const& h) { std::ostringstream s; auto hashes=h.info_hashes(); if(hashes.has_v2()) s<<hashes.v2; else s<<hashes.v1; return s.str(); }
static void atomicWrite(std::filesystem::path const& path, std::vector<char> const& bytes) { auto temporary=path; temporary += L".tmp"; { std::ofstream f(temporary,std::ios::binary|std::ios::trunc); f.write(bytes.data(),bytes.size()); if(!f) throw std::runtime_error("Cannot write resume data"); } if(!MoveFileExW(temporary.c_str(),path.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)) throw std::runtime_error("Cannot replace resume data (Windows error "+std::to_string(GetLastError())+")"); }
Engine::Engine(std::filesystem::path directory):session(settings()),root(std::move(directory)) {
 std::filesystem::create_directories(root);
 std::ifstream order(root/L"queue.txt"); std::string id; std::vector<std::string> ids; while(std::getline(order,id)) ids.push_back(id);
 for(auto const& entry:std::filesystem::directory_iterator(root)) if(entry.path().extension()==L".resume") { auto name=entry.path().stem().string(); if(std::find(ids.begin(),ids.end(),name)==ids.end()) ids.push_back(name); }
 for(auto const& name:ids) try { std::ifstream f(root/(name+".resume"),std::ios::binary); if(!f) continue; std::vector<char> bytes((std::istreambuf_iterator<char>(f)),{}); auto p=lt::read_resume_data(bytes); auto h=session.add_torrent(p); Item i{h,name}; std::ifstream state(root/(i.key+".state")); state>>i.stopped>>i.selectPending; items.push_back(i); } catch(std::exception const& e) { errors.push_back(e.what()); }
}
lt::torrent_handle Engine::add(std::string const& source,std::string const& destination,bool select) {
 lt::add_torrent_params p;
 if(source.rfind("magnet:",0)==0) p=lt::parse_magnet_uri(source); else p.ti=std::make_shared<lt::torrent_info>(source);
 p.save_path=destination;
 if(select) { p.flags &= ~lt::torrent_flags::auto_managed; p.flags &= ~lt::torrent_flags::paused; p.flags |= lt::torrent_flags::default_dont_download; }
 auto h=session.add_torrent(p); for(auto const& i:items) if(i.handle==h) return h;
 items.push_back({h,key(h),false,select}); request(items.back()); persistOrder(); return h;
}
void Engine::request(Item const& i) { i.handle.save_resume_data(lt::torrent_handle::save_info_dict); ++outstanding; }
void Engine::persistOrder() { std::string order; for(auto const& i:items) { order+=i.key+"\n"; std::string state=std::to_string(i.stopped)+" "+std::to_string(i.selectPending); atomicWrite(root/(i.key+".state"),{state.begin(),state.end()}); } atomicWrite(root/L"queue.txt",{order.begin(),order.end()}); }
void Engine::save() { persistOrder(); if(outstanding==0) for(auto const& i:items) request(i); }
void Engine::tick() {
 std::vector<lt::alert*> alerts; session.pop_alerts(&alerts);
 for(auto a:alerts) {
  if(auto r=lt::alert_cast<lt::save_resume_data_alert>(a)) { --outstanding; auto item=std::find_if(items.begin(),items.end(),[&](auto const& i){return i.handle==r->handle;}); if(item!=items.end()) try { atomicWrite(root/(item->key+".resume"),lt::write_resume_data_buf(r->params)); } catch(std::exception const& e) {errors.push_back(e.what());} }
  else if(auto failure=lt::alert_cast<lt::save_resume_data_failed_alert>(a)) { --outstanding; errors.push_back(failure->message()); }
  else if(auto torrentError=lt::alert_cast<lt::torrent_error_alert>(a)) errors.push_back(torrentError->message());
 }
}
void Engine::remove(lt::torrent_handle h) { auto item=std::find_if(items.begin(),items.end(),[&](auto const& i){return i.handle==h;}); if(item==items.end())return; auto id=item->key; session.remove_torrent(h); std::erase_if(items,[&](auto const& i){return i.handle==h;}); std::filesystem::remove(root/(id+".resume")); std::filesystem::remove(root/(id+".state")); persistOrder(); }
void Engine::shutdown() {
 auto drain=[&] {auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(20);while(outstanding>0) {session.wait_for_alert(std::chrono::milliseconds(100));tick();if(std::chrono::steady_clock::now()>deadline)throw std::runtime_error("Resume save timed out; previous saved state is preserved");}};
 drain();save();drain();
}
