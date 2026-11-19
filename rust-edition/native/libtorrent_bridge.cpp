#include <libtorrent/alert_types.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/write_resume_data.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <exception>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace lt = libtorrent;

#if defined(_WIN32)
#define RE_EXPORT __declspec(dllexport)
#else
#define RE_EXPORT __attribute__((visibility("default")))
#endif

namespace {
template <class Function>
auto checked(char* error, std::size_t capacity, Function operation) noexcept
    -> decltype(operation()) {
    try {
        return operation();
    } catch (std::exception const& failure) {
        std::snprintf(error, capacity, "%s", failure.what());
    } catch (...) {
        std::snprintf(error, capacity, "Unknown libtorrent error");
    }
    return {};
}
}

extern "C" {
RE_EXPORT lt::session* re_session_open(int downloads, int seeds, int active, int connections,
                                      bool offline, char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        lt::settings_pack options;
        options.set_int(lt::settings_pack::active_downloads, downloads);
        options.set_int(lt::settings_pack::active_seeds, seeds);
        options.set_int(lt::settings_pack::active_limit, active);
        options.set_int(lt::settings_pack::connections_limit, connections);
        options.set_str(lt::settings_pack::user_agent, "Torrent/0.1");
        if (offline) {
            options.set_str(lt::settings_pack::listen_interfaces, "127.0.0.1:0");
            options.set_bool(lt::settings_pack::enable_dht, false);
            options.set_bool(lt::settings_pack::enable_lsd, false);
            options.set_bool(lt::settings_pack::enable_upnp, false);
            options.set_bool(lt::settings_pack::enable_natpmp, false);
        }
        return new lt::session(std::move(options));
    });
}

RE_EXPORT void re_session_close(lt::session* session) noexcept { delete session; }

RE_EXPORT lt::torrent_handle* re_add(lt::session* session, char const* source,
                                     char const* destination, bool magnet, bool select,
                                     char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        lt::add_torrent_params params;
        if (magnet) {
            params = lt::parse_magnet_uri(source);
        } else {
            params.ti = std::make_shared<lt::torrent_info>(std::string(source));
        }
        params.save_path = destination;
        if (select) {
            params.flags &= ~lt::torrent_flags::auto_managed;
            params.flags &= ~lt::torrent_flags::paused;
            params.flags |= lt::torrent_flags::default_dont_download;
        }
        auto handle = std::make_unique<lt::torrent_handle>();
        *handle = session->add_torrent(params);
        return handle.release();
    });
}

RE_EXPORT void re_handle_close(lt::torrent_handle* handle) noexcept { delete handle; }

RE_EXPORT bool re_remove(lt::session* session, lt::torrent_handle* handle,
                         char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        session->remove_torrent(*handle);
        return true;
    });
}

RE_EXPORT bool re_pause(lt::torrent_handle* handle, bool pause, bool auto_managed,
                        char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        if (pause) {
            handle->unset_flags(lt::torrent_flags::auto_managed);
            handle->pause();
        } else {
            if (auto_managed) handle->set_flags(lt::torrent_flags::auto_managed);
            handle->resume();
        }
        return true;
    });
}

RE_EXPORT bool re_paused(lt::torrent_handle* handle, bool* result,
                         char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        *result = bool(handle->status().flags & lt::torrent_flags::paused);
        return true;
    });
}

RE_EXPORT bool re_read_policy(lt::session* session, int* values,
                              char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        auto settings = session->get_settings();
        values[0] = settings.get_int(lt::settings_pack::active_downloads);
        values[1] = settings.get_int(lt::settings_pack::active_seeds);
        values[2] = settings.get_int(lt::settings_pack::active_limit);
        values[3] = settings.get_int(lt::settings_pack::connections_limit);
        return true;
    });
}
RE_EXPORT bool re_file_priorities(lt::torrent_handle* handle, std::uint8_t* values,
                                  std::size_t length, std::size_t* count,
                                  char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        auto priorities = handle->get_file_priorities();
        *count = priorities.size();
        if (values != nullptr) {
            if (length != priorities.size()) throw std::runtime_error("File count changed");
            for (std::size_t index = 0; index < length; ++index)
                values[index] = static_cast<std::uint8_t>(priorities[index]);
        }
        return true;
    });
}

RE_EXPORT bool re_select_files(lt::torrent_handle* handle, std::uint8_t const* values,
                               std::size_t length, char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        auto metadata = handle->torrent_file();
        if (!metadata || static_cast<std::size_t>(metadata->num_files()) != length)
            throw std::runtime_error("File selection does not match torrent metadata");
        std::vector<lt::download_priority_t> priorities;
        priorities.reserve(length);
        for (std::size_t index = 0; index < length; ++index) {
            if (values[index] > 7) throw std::runtime_error("Invalid file priority");
            priorities.emplace_back(values[index]);
        }
        handle->prioritize_files(priorities);
        handle->unset_flags(lt::torrent_flags::default_dont_download);
        handle->set_flags(lt::torrent_flags::auto_managed);
        handle->resume();
        return true;
    });
}

RE_EXPORT lt::torrent_handle* re_restore(lt::session* session, char const* bytes,
                                         std::size_t length, char* error,
                                         std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        auto params = lt::read_resume_data(lt::span<char const>(bytes, static_cast<std::ptrdiff_t>(length)));
        auto handle = std::make_unique<lt::torrent_handle>();
        *handle = session->add_torrent(std::move(params));
        return handle.release();
    });
}

// Only one Rust caller drains this session. Save requests are serialized.
RE_EXPORT bool re_checkpoint(lt::session* session, lt::torrent_handle* handle,
                              void (*receive)(void*, char const*, std::size_t),
                              void* context, char* error, std::size_t capacity) noexcept {
    return checked(error, capacity, [&] {
        using namespace std::chrono;
        handle->save_resume_data(lt::torrent_handle::save_info_dict);
        auto const deadline = steady_clock::now() + seconds(15);
        std::vector<lt::alert*> alerts;
        while (steady_clock::now() < deadline) {
            session->wait_for_alert(milliseconds(100));
            session->pop_alerts(&alerts);
            for (auto const* alert : alerts) {
                auto const* saved = lt::alert_cast<lt::save_resume_data_alert>(alert);
                if (saved && saved->handle == *handle) {
                    auto const bytes = lt::write_resume_data_buf(saved->params);
                    receive(context, bytes.data(), bytes.size());
                    return true;
                }
                auto const* failed = lt::alert_cast<lt::save_resume_data_failed_alert>(alert);
                if (failed && failed->handle == *handle) throw std::runtime_error(failed->message());
            }
        }
        throw std::runtime_error("Timed out waiting for libtorrent resume data");
    });
}

}
