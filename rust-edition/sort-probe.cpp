#include "file_list_sort.h"
#include <charconv>
#include <chrono>
#include <iostream>
#include <random>
#include <string_view>

extern "C" void torrent_re_sort_by_size(
 const std::int64_t*, std::int32_t*, std::size_t, std::uint8_t);

static std::vector<int> sorted(const std::vector<std::int64_t>& sizes, bool ascending, bool rust) {
    if (!rust) {
        return filelist::sortedIndices(std::vector<std::wstring>(sizes.size()), sizes, 2, ascending);
    }
    std::vector<int> indices(sizes.size());
    std::iota(indices.begin(), indices.end(), 0);
    torrent_re_sort_by_size(sizes.data(), indices.data(), indices.size(), ascending);
    return indices;
}

int main(int argc, char** argv) {
    if (argc < 3) return 2;
    const std::string_view mode = argv[1], direction = argv[2];
    const bool ascending = direction != "desc";
    if (mode == "bench") {
        std::mt19937 random(42);
        std::vector<std::int64_t> sizes(10000);
        for (auto& size : sizes) size = random() % 1000;
        std::vector<std::wstring> names(sizes.size());
        for (bool rust : {false, true}) {
            const auto start = std::chrono::steady_clock::now();
            std::uint64_t checksum = 0;
            for (int repeat = 0; repeat < 200; ++repeat) {
                const bool ascendingSample = repeat % 2 == 0;
                std::vector<int> order;
                if (rust) {
                    order = sorted(sizes, ascendingSample, true);
                } else {
                    order = filelist::sortedIndices(names, sizes, 2, ascendingSample);
                }
                checksum += order[repeat];
            }
            const auto elapsed = std::chrono::steady_clock::now() - start;
            std::cout << (rust ? "Rust" : "C++") << " sort (200 x 10000 files): "
                      << std::chrono::duration<double, std::milli>(elapsed).count()
                      << " ms, checksum " << checksum << '\n';
        }
        for (int count = 0; count < 512; ++count) {
            sizes.resize(count);
            for (auto& size : sizes) size = random() % 23;
            for (bool ascendingSample : {false, true}) {
                if (sorted(sizes, ascendingSample, false) != sorted(sizes, ascendingSample, true))
                    return 1;
            }
        }
        return 0;
    }
    std::vector<std::int64_t> sizes;
    for (int i = 3; i < argc; ++i) {
        const std::string_view text = argv[i];
        std::int64_t size;
        const auto parsed = std::from_chars(text.data(), text.data() + text.size(), size);
        if (parsed.ec != std::errc{} || parsed.ptr != text.data() + text.size()) return 2;
        sizes.push_back(size);
    }
    for (int index : sorted(sizes, ascending, mode == "rust")) std::cout << index << '\n';
}
