#include "engine.h"
#include <libtorrent/create_torrent.hpp>
#include <libtorrent/bencode.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <libtorrent/settings_pack.hpp>
#include <fstream>
#include <iostream>
#include <thread>
#include <chrono>
#include <stdexcept>
static void require(bool condition,char const* message) {if(!condition) throw std::runtime_error(message);}
int main() {
 auto root=std::filesystem::temp_directory_path()/("Torrent-test-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
 try {
  std::filesystem::create_directories(root/"seed");
  {std::ofstream f(root/"seed"/"sample.bin",std::ios::binary);std::string bytes(65536,'T');f.write(bytes.data(),bytes.size());}
  lt::file_storage files; files.add_file("sample.bin",65536); lt::create_torrent creator(files,16384,lt::create_torrent::v1_only);lt::set_piece_hashes(creator,utf8((root/"seed").wstring()));
  std::vector<char> bytes;lt::bencode(std::back_inserter(bytes),creator.generate()); auto torrent=root/"sample.torrent"; {std::ofstream f(torrent,std::ios::binary);f.write(bytes.data(),bytes.size());}
  std::string magnet;
  lt::settings_pack seedSettings;seedSettings.set_str(lt::settings_pack::listen_interfaces,"127.0.0.1:0");seedSettings.set_bool(lt::settings_pack::enable_dht,false);seedSettings.set_bool(lt::settings_pack::enable_lsd,false);seedSettings.set_bool(lt::settings_pack::enable_upnp,false);seedSettings.set_bool(lt::settings_pack::enable_natpmp,false);
  lt::session seed(seedSettings);lt::add_torrent_params seedParams;seedParams.ti=std::make_shared<lt::torrent_info>(utf8(torrent.wstring()));seedParams.save_path=utf8((root/"seed").wstring());seedParams.flags|=lt::torrent_flags::seed_mode;seedParams.flags&=~(lt::torrent_flags::paused|lt::torrent_flags::auto_managed);auto seedHandle=seed.add_torrent(seedParams);
  auto waitFor=[&](auto predicate,char const* message) {auto deadline=std::chrono::steady_clock::now()+std::chrono::seconds(25);while(!predicate()){if(std::chrono::steady_clock::now()>deadline)throw std::runtime_error(message);std::this_thread::sleep_for(std::chrono::milliseconds(100));}};
  waitFor([&]{return seed.listen_port()!=0;},"loopback seed did not listen");
  {
   Engine e(root/"state");auto h=e.add(utf8(torrent.wstring()),utf8((root/"download").wstring()),true);
   require(e.items.size()==1,"torrent was not added");require(bool(h.torrent_file()),"metadata missing");
   std::vector<lt::download_priority_t> priorities{lt::top_priority};h.prioritize_files(priorities);h.unset_flags(lt::torrent_flags::auto_managed);h.pause();e.items[0].selectPending=false;e.items[0].stopped=true;
   h.unset_flags(lt::torrent_flags::default_dont_download);h.resume();h.connect_peer(lt::tcp::endpoint(lt::address_v4::loopback(),seed.listen_port()));waitFor([&]{return h.status().is_seeding;},"loopback .torrent download did not finish");h.pause();
   magnet=lt::make_magnet_uri(h);e.shutdown();require(e.errors.empty(),"resume save failed");
  }
  {
   Engine e(root/"state");require(e.items.size()==1,"state not restored");require(e.items[0].stopped,"stop state lost");auto h=e.items[0].handle;require(h.get_file_priorities()[0]==lt::top_priority,"file priority lost");
   require(bool(h.status().flags&lt::torrent_flags::paused),"pause state lost");h.resume();h.pause();
   e.remove(h);e.shutdown();require(e.items.empty(),"removal failed");
  }
  {
   Engine e(root/"state");require(e.items.empty(),"removed torrent resurrected");auto h=e.add(magnet,utf8((root/"magnet").wstring()),true);require(h.is_valid(),"magnet not added");require(e.items[0].selectPending,"magnet selection flag missing");e.shutdown();
  }
  {Engine e(root/"state");require(e.items.size()==1&&e.items[0].selectPending,"magnet state not restored");e.remove(e.items[0].handle);e.shutdown();}
  {
   Engine e(root/"magnet-state");auto h=e.add(magnet,utf8((root/"magnet-transfer").wstring()),true);h.connect_peer(lt::tcp::endpoint(lt::address_v4::loopback(),seed.listen_port()));waitFor([&]{return bool(h.torrent_file());},"magnet metadata not fetched");require(h.status().total_wanted_done==0,"payload downloaded before file selection");h.prioritize_files({lt::top_priority});h.unset_flags(lt::torrent_flags::default_dont_download);h.resume();waitFor([&]{return h.status().is_seeding;},"magnet transfer did not finish");e.shutdown();
   std::ifstream file(root/"magnet-transfer"/"sample.bin",std::ios::binary);std::string content((std::istreambuf_iterator<char>(file)),{});require(content==std::string(65536,'T'),"downloaded payload differs from seed");
  }
  seed.remove_torrent(seedHandle);std::cout<<"torrent, magnet, loopback transfer, priorities, pause, stop, persistence, removal: PASS\nTest data: "<<root<<'\n';return 0;
 } catch(std::exception const& e) {std::cerr<<e.what()<<"\nTest data: "<<root<<'\n';return 1;}
}
