#include "camera_bridge.h"
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

int main(int argc, char **argv) {
  if (argc != 13 && argc != 14) {
    std::cerr << "Usage: camera-native-check DEVICE W H FPS X Y CW CH OW OH "
                 "FRAMES ROTATION [NEW.uyvy]\n";
    return 2;
  }
  try {
    const auto number = [&](int i) {
      size_t used = 0;
      auto n = std::stoul(argv[i], &used);
      if (used != std::string(argv[i]).size() || n > UINT32_MAX)
        throw std::runtime_error("invalid integer");
      return uint32_t(n);
    };
    MdK1CameraConfig c{sizeof(c), MD_K1_CAMERA_ABI, argv[1],   number(2),
                       number(3), number(4),        number(5), number(6),
                       number(7), number(8),        number(9), number(10),
                       number(12)};
    const auto count = number(11);
    if (!count || count > 10000)
      throw std::runtime_error("frames must be 1..10000");
    char error[1024]{};
    void *raw = nullptr;
    if (md_k1_camera_open(&c, &raw, error, sizeof(error)))
      throw std::runtime_error(error);
    std::unique_ptr<void, decltype(&md_k1_camera_close)> camera(
        raw, md_k1_camera_close);
    std::vector<uint8_t> bytes(size_t(c.output_width) * c.output_height * 2);
    uint64_t first_pts = 0, last_pts = 0, wait = 0, image = 0, pack = 0;
    auto start = std::chrono::steady_clock::now();
    for (uint32_t i = 0; i < count + 3; ++i) {
      if (i == 3)
        start = std::chrono::steady_clock::now();
      MdK1CameraFrame f{};
      if (md_k1_camera_read(camera.get(), bytes.data(), bytes.size(), 5000, &f,
                            error, sizeof(error)))
        throw std::runtime_error(error);
      if (i < 3)
        continue;
      if (i == 3) {
        first_pts = f.pts_ns;
        if (argc == 14) {
          FILE *out = std::fopen(argv[13], "wbx");
          if (!out)
            throw std::runtime_error("dump file must be new and writable");
          auto written = std::fwrite(bytes.data(), 1, f.bytes, out);
          auto rc = std::fclose(out);
          if (written != f.bytes || rc)
            throw std::runtime_error("dump write failed");
        }
      }
      last_pts = f.pts_ns;
      wait += f.wait_us;
      image += f.image_us;
      pack += f.pack_us;
      std::cout << "{\"event\":\"native-frame\",\"index\":" << i - 3
                << ",\"pts_ns\":" << f.pts_ns << ",\"width\":" << f.width
                << ",\"height\":" << f.height << ",\"wait_us\":" << f.wait_us
                << ",\"image_us\":" << f.image_us
                << ",\"pack_us\":" << f.pack_us << "}\n";
    }
    const double seconds =
        std::chrono::duration<double>(std::chrono::steady_clock::now() - start)
            .count();
    std::cout << "{\"event\":\"native-summary\",\"frames\":" << count
              << ",\"consumed_fps\":" << count / seconds << ",\"pts_fps\":"
              << (last_pts > first_pts
                      ? (count - 1) * 1e9 / double(last_pts - first_pts)
                      : 0)
              << ",\"wait_mean_ms\":" << wait / double(count) / 1000
              << ",\"image_mean_ms\":" << image / double(count) / 1000
              << ",\"pack_mean_ms\":" << pack / double(count) / 1000 << "}\n";
  } catch (const std::exception &e) {
    std::cerr << e.what() << '\n';
    return 1;
  }
  return 0;
}
