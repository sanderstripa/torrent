#pragma once
#include <libtorrent/session.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <filesystem>
#include <string>
#include <vector>
namespace lt = libtorrent;
struct Item { lt::torrent_handle handle; std::string key; bool stopped=false; bool selectPending=false; };
class Engine {
public:
 explicit Engine(std::filesystem::path directory);
 lt::torrent_handle add(std::string const& source, std::string const& destination, bool select);
 void tick();
 void save();
 void shutdown();
 void remove(lt::torrent_handle h);
 std::vector<Item> items;
 std::vector<std::string> errors;
private:
 lt::session session;
 std::filesystem::path root;
 int outstanding=0;
 void request(Item const& item);
 void persistOrder();
};
std::string utf8(std::wstring const& value);
std::wstring wide(std::string const& value);
