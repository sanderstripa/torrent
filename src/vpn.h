#pragma once
#include <windows.h>
#include <filesystem>
#include <string>

class Engine;

class VpnManager {
public:
 enum class State { disconnected, connecting, connected, blocked };
 static constexpr int port=17891;

 explicit VpnManager(std::filesystem::path dataDirectory);
 ~VpnManager();
 bool loadKey();
 bool saveKey(std::wstring const& value);
 bool start(Engine& engine,std::wstring const& value);
 void stop(Engine& engine);
 void shutdown(Engine& engine);
 void tick();
 std::wstring const& key() const {return connectionKey;}
 std::wstring const& error() const {return lastError;}
 State state() const {return currentState;}
 bool isolated() const {return isolationEnabled;}

private:
 std::filesystem::path root,keyFile,statusFile,temporaryKeyFile;
 std::wstring connectionKey,lastError;
 State currentState=State::disconnected;
 HANDLE process=nullptr,stdinWrite=nullptr;
 bool isolationEnabled=false;
 void closeProcess(bool force);
};
