#include "vpn.h"
#include "engine.h"
#include <wincrypt.h>
#include <fstream>
#include <iterator>
#include <vector>

static std::wstring quote(std::filesystem::path const& path) {
 return L"\""+path.wstring()+L"\"";
}

static std::filesystem::path executableDirectory() {
 std::wstring buffer(32768,L'\0');
 DWORD length=GetModuleFileNameW(nullptr,buffer.data(),DWORD(buffer.size()));
 if(length==0||length>=buffer.size())return {};
 buffer.resize(length);
 return std::filesystem::path(buffer).parent_path();
}

static bool atomicBytes(std::filesystem::path const& path,std::vector<unsigned char> const& bytes) {
 auto temporary=path;temporary+=L".tmp";
 {std::ofstream output(temporary,std::ios::binary|std::ios::trunc);output.write(reinterpret_cast<char const*>(bytes.data()),std::streamsize(bytes.size()));output.flush();if(!output)return false;}
 return MoveFileExW(temporary.c_str(),path.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)!=FALSE;
}

VpnManager::VpnManager(std::filesystem::path dataDirectory):root(std::move(dataDirectory)/L"VPN") {
 std::filesystem::create_directories(root);
 keyFile=root/L"connection-key.dat";
 statusFile=root/L"status.txt";
 temporaryKeyFile=root/L"connection-key.tmp";
}

VpnManager::~VpnManager() {
 closeProcess(true);
}

bool VpnManager::loadKey() {
 try {
  std::ifstream input(keyFile,std::ios::binary);
  if(!input)return true;
  std::vector<unsigned char> encrypted((std::istreambuf_iterator<char>(input)),{});
  DATA_BLOB source{DWORD(encrypted.size()),encrypted.data()},plain{};
  if(!CryptUnprotectData(&source,nullptr,nullptr,nullptr,nullptr,CRYPTPROTECT_UI_FORBIDDEN,&plain))return false;
  std::string utf8Value(reinterpret_cast<char*>(plain.pbData),plain.cbData);
  LocalFree(plain.pbData);
  connectionKey=wide(utf8Value);
  return true;
 }catch(...){return false;}
}

bool VpnManager::saveKey(std::wstring const& value) {
 auto text=utf8(value);
 DATA_BLOB source{DWORD(text.size()),reinterpret_cast<unsigned char*>(text.data())},encrypted{};
 if(!CryptProtectData(&source,L"Torrent isolated VPN key",nullptr,nullptr,nullptr,CRYPTPROTECT_UI_FORBIDDEN,&encrypted))return false;
 std::vector<unsigned char> bytes(encrypted.pbData,encrypted.pbData+encrypted.cbData);
 LocalFree(encrypted.pbData);
 if(!atomicBytes(keyFile,bytes))return false;
 connectionKey=value;
 return true;
}

void VpnManager::closeProcess(bool force) {
 if(stdinWrite){CloseHandle(stdinWrite);stdinWrite=nullptr;}
 if(process){
  DWORD wait=WaitForSingleObject(process,force?1500:5000);
  if(wait==WAIT_TIMEOUT){TerminateProcess(process,1);WaitForSingleObject(process,2000);}
  CloseHandle(process);process=nullptr;
 }
 std::error_code ignored;
 std::filesystem::remove(temporaryKeyFile,ignored);
 std::filesystem::remove(statusFile,ignored);
}

bool VpnManager::start(Engine& engine,std::wstring const& value) {
 closeProcess(true);
 lastError.clear();
 isolationEnabled=true;
 currentState=State::connecting;
 engine.enableVpnProxy(port);
 auto text=utf8(value);
 if(text.empty()||(text.rfind("vless://",0)!=0&&text.rfind("sudoku://",0)!=0)){
  lastError=L"Нужен ключ vless:// или sudoku://";
  currentState=State::blocked;
  return false;
 }
 {std::ofstream output(temporaryKeyFile,std::ios::binary|std::ios::trunc);output.write(text.data(),std::streamsize(text.size()));if(!output){lastError=L"Не удалось подготовить ключ VPN";currentState=State::blocked;return false;}}
 std::error_code ignored;std::filesystem::remove(statusFile,ignored);
 auto host=executableDirectory()/L"vpn"/L"TorrentVpnHost.exe";
 if(!std::filesystem::exists(host)){lastError=L"Компонент VPN не установлен";currentState=State::blocked;return false;}
 SECURITY_ATTRIBUTES attributes{sizeof(attributes),nullptr,TRUE};
 HANDLE stdinRead=nullptr;
 if(!CreatePipe(&stdinRead,&stdinWrite,&attributes,0)){lastError=L"Не удалось создать канал VPN";currentState=State::blocked;return false;}
 SetHandleInformation(stdinWrite,HANDLE_FLAG_INHERIT,0);
 HANDLE nullOutput=CreateFileW(L"NUL",GENERIC_WRITE,FILE_SHARE_READ|FILE_SHARE_WRITE,&attributes,OPEN_EXISTING,0,nullptr);
 std::wstring command=quote(host)+L" --run "+quote(temporaryKeyFile)+L" "+quote(statusFile)+L" "+std::to_wstring(port);
 STARTUPINFOW startup{sizeof(startup)};startup.dwFlags=STARTF_USESTDHANDLES;startup.hStdInput=stdinRead;startup.hStdOutput=nullOutput;startup.hStdError=nullOutput;
 PROCESS_INFORMATION information{};
 BOOL created=CreateProcessW(host.c_str(),command.data(),nullptr,nullptr,TRUE,CREATE_NO_WINDOW,nullptr,host.parent_path().c_str(),&startup,&information);
 CloseHandle(stdinRead);if(nullOutput!=INVALID_HANDLE_VALUE)CloseHandle(nullOutput);
 if(!created){CloseHandle(stdinWrite);stdinWrite=nullptr;lastError=L"Не удалось запустить VPN";currentState=State::blocked;return false;}
 CloseHandle(information.hThread);process=information.hProcess;connectionKey=value;return true;
}

void VpnManager::stop(Engine& engine) {
 closeProcess(false);
 isolationEnabled=false;
 currentState=State::disconnected;
 lastError.clear();
 engine.disableVpnProxy();
}

void VpnManager::shutdown(Engine& engine) {
 (void)engine;
 closeProcess(false);
 currentState=State::disconnected;
}

void VpnManager::tick() {
 if(!process)return;
 if(WaitForSingleObject(process,0)==WAIT_OBJECT_0){
  closeProcess(false);
  if(isolationEnabled){currentState=State::blocked;if(lastError.empty())lastError=L"VPN остановлен, трафик Torrent заблокирован";}
  return;
 }
 if(currentState!=State::connecting)return;
 std::ifstream input(statusFile,std::ios::binary);if(!input)return;
 std::string value((std::istreambuf_iterator<char>(input)),{});input.close();
 std::error_code ignored;std::filesystem::remove(statusFile,ignored);
 if(value=="ready"){currentState=State::connected;lastError.clear();return;}
 if(value.rfind("error:",0)==0){lastError=wide(value.substr(6));currentState=State::blocked;}
}
