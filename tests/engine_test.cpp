#include "engine.h"
#include <libtorrent/create_torrent.hpp>
#include <libtorrent/bencode.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
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
  {
   Engine e(root/"state");auto h=e.add(utf8(torrent.wstring()),utf8((root/"download").wstring()),true);
   require(e.items.size()==1,"torrent was not added");require(bool(h.torrent_file()),"metadata missing");
   std::vector<lt::download_priority_t> priorities{lt::top_priority};h.prioritize_files(priorities);h.unset_flags(lt::torrent_flags::auto_managed);h.pause();e.items[0].selectPending=false;e.items[0].stopped=true;
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
  std::filesystem::remove_all(root);std::cout<<"torrent, magnet, priorities, pause, stop, persistence, removal: PASS\n";return 0;
 } catch(std::exception const& e) {std::cerr<<e.what()<<"\nTest data: "<<root<<'\n';return 1;}
}
