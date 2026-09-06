// Board-only checks of the production Processor; compiled out of the bridge.
#include <fstream>
#include <iostream>
#include <vector>

static uint8_t limited_y(int v) {
  return uint8_t(std::lround(16.0 + v * 219.0 / 255.0));
}
static uint8_t limited_uv(int v) {
  return uint8_t(std::lround(128.0 + (v - 128) * 224.0 / 255.0));
}

static void check_image(uint32_t rotation, bool crop, bool letterbox,
                        bool padded = false, bool split = false) {
  MdK1CameraConfig c{sizeof(c),
                     MD_K1_CAMERA_ABI,
                     "/unused",
                     640,
                     480,
                     30,
                     crop ? 32u : 0u,
                     crop ? 24u : 0u,
                     crop ? 576u : 640u,
                     crop ? 432u : 480u,
                     crop ? 576u : 640u,
                     letterbox ? 360u
                     : crop    ? 432u
                               : 480u,
                     rotation};
  Image input(c.width + (padded ? 16 : 0), c.height);
  {
    Sync sync(input.fd(), DMA_BUF_SYNC_WRITE);
    auto y = input.y(), uv = input.uv();
    for (int row = 0; row < y.rows; ++row)
      for (int x = 0; x < y.cols; ++x)
        y.at<uint8_t>(row, x) = letterbox ? 180 : (x * 3 + row * 7) % 256;
    for (int row = 0; row < uv.rows; ++row)
      for (int x = 0; x < uv.cols; ++x)
        uv.at<cv::Vec2b>(row, x) =
            letterbox
                ? cv::Vec2b(60, 200)
                : cv::Vec2b((x * 3 + row * 5) % 256, (x * 7 + row * 3) % 256);
  }
  Processor processor(c);
  const auto &g = processor.geometry();
  std::vector<uint8_t> actual(size_t(g.width) * g.height * 2);
  MdK1CameraFrame meta{};
  auto frame = input.frame;
  frame.stCommFrameInfo.u32Width = c.width;
  std::unique_ptr<Image> second;
  if (split) {
    second = std::make_unique<Image>(c.width + (padded ? 16 : 0), c.height);
    Sync src(input.fd(), DMA_BUF_SYNC_READ),
        dst(second->fd(), DMA_BUF_SYNC_WRITE);
    input.uv().copyTo(second->uv());
    frame.stVFrame.u32Fd[1] = second->fd();
    frame.stVFrame.ulPlaneVirAddr[1] = second->frame.stVFrame.ulPlaneVirAddr[1];
  }
  processor.run(frame, actual.data(), actual.size(), meta);
  size_t differences = 0;
  int maximum = 0;
  Sync read(input.fd(), DMA_BUF_SYNC_READ);
  for (uint32_t row = 0; row < g.height; ++row)
    for (uint32_t x = 0; x < g.width; ++x) {
      uint32_t sx = x, sy = row;
      if (rotation == 90) {
        sx = row;
        sy = c.output_height - 1 - x;
      }
      if (rotation == 180) {
        sx = c.output_width - 1 - x;
        sy = c.output_height - 1 - row;
      }
      if (rotation == 270) {
        sx = c.output_width - 1 - row;
        sy = x;
      }
      int y = 0, u = 128, v = 128;
      if (letterbox) {
        if (sx >= g.destination.u16X &&
            sx < g.destination.u16X + g.destination.u16W &&
            sy >= g.destination.u16Y &&
            sy < g.destination.u16Y + g.destination.u16H) {
          y = 180;
          u = 60;
          v = 200;
        }
      } else {
        sx += c.crop_x;
        sy += c.crop_y;
        y = input.y().at<uint8_t>(sy, sx);
        auto uv = input.uv().at<cv::Vec2b>(sy / 2, sx / 2);
        u = uv[0];
        v = uv[1];
      }
      const uint8_t expected[] = {limited_uv(x % 2 ? v : u), limited_y(y)};
      for (size_t b = 0; b < 2; ++b) {
        int delta = std::abs(int(actual[(size_t(row) * g.width + x) * 2 + b]) -
                             expected[b]);
        if (delta)
          ++differences;
        maximum = std::max(maximum, delta);
      }
    }
  std::cout << "image-test rotation=" << rotation << " crop=" << crop
            << " letterbox=" << letterbox << " padded=" << padded
            << " split=" << split << " bytes=" << actual.size()
            << " differences=" << differences << " max=" << maximum
            << std::endl;
  require(differences == 0, "synthetic image differs from the exact reference");
}

static void invalid_inputs() {
  MdK1CameraConfig good{sizeof(good),
                        MD_K1_CAMERA_ABI,
                        "/unused",
                        640,
                        480,
                        30,
                        0,
                        0,
                        640,
                        480,
                        640,
                        480,
                        0};
  auto rejected = [](const MdK1CameraConfig &c) {
    void *handle = nullptr;
    char error[256]{};
    require(md_k1_camera_open(&c, &handle, error, sizeof(error)) == -1 &&
                !handle && error[0],
            "invalid configuration was not rejected");
  };
  auto c = good;
  c.abi = 999;
  rejected(c);
  c = good;
  c.struct_size = 0;
  rejected(c);
  c = good;
  c.width = UINT32_MAX;
  rejected(c);
  c = good;
  c.crop_x = 1;
  rejected(c);
  c = good;
  c.crop_height = 482;
  rejected(c);
  c = good;
  c.output_width = 16;
  c.output_height = 12;
  rejected(c); // more than 8x downscale
  c = good;
  c.output_width = 640;
  c.output_height = 358;
  rejected(c); // odd inner rectangle
  c = good;
  c.rotation = 45;
  rejected(c);
  c = good;
  c.fps = 0;
  rejected(c);
  c = good;
  c.device = "relative";
  rejected(c);
  Image image(640, 480);
  Processor processor(good);
  MdK1CameraFrame meta{};
  std::vector<uint8_t> output(640 * 480 * 2);
  auto fail = [&](VideoFrameInfo frame, size_t size) {
    bool threw = false;
    try {
      processor.run(frame, output.data(), size, meta);
    } catch (const std::exception &) {
      threw = true;
    }
    require(threw, "invalid frame/buffer was not rejected");
  };
  fail(image.frame, output.size() - 1);
  auto frame = image.frame;
  frame.stVFrame.u32PlaneSize[0] = 1;
  fail(frame, output.size());
  frame = image.frame;
  frame.stCommFrameInfo.ePixelFormat = MPP_PIXEL_FORMAT_UYVY;
  fail(frame, output.size());
  std::cout << "13 invalid config/ABI/frame checks PASS\n";
}

// Decode the SAME saved JPEG used for software comparison. No camera is opened.
static void saved_jpeg(const char *path, const char *dump) {
  std::ifstream file(path, std::ios::binary);
  require(bool(file), "cannot open input JPEG");
  std::vector<uint8_t> jpeg((std::istreambuf_iterator<char>(file)), {});
  require(!jpeg.empty() && jpeg.size() < 16 * 1024 * 1024, "invalid JPEG size");
  checked(SYS_Init(), "SYS_Init");
  checked(VB_Init(), "VB_Init");
  checked(VDEC_Init(), "VDEC_Init");
  VdecChnAttr attr{};
  attr.eCodecType = MPP_STREAM_CODEC_MJPEG;
  attr.eOutputPixelFormat = MPP_PIXEL_FORMAT_NV12;
  attr.u32Width = 4000;
  attr.u32Height = 1200;
  attr.u32BufCnt = 6;
  checked(VDEC_CreateChn(0, &attr), "VDEC_CreateChn");
  checked(VDEC_EnableChn(0), "VDEC_EnableChn");
  StreamBufferInfo stream{};
  stream.pu8Addr = jpeg.data();
  stream.u32Size = jpeg.size();
  stream.u64PTS = 1000000;
  stream.eCodecType = MPP_STREAM_CODEC_MJPEG;
  stream.bKeyFrame = MPP_TRUE;
  stream.u32Width = 4000;
  stream.u32Height = 1200;
  checked(VDEC_SendStream(0, &stream, 1000), "VDEC_SendStream");
  VideoFrameInfo frame{};
  checked(VDEC_GetFrame(0, &frame, 5000), "VDEC_GetFrame saved JPEG");
  MdK1CameraConfig c{sizeof(c), MD_K1_CAMERA_ABI,
                     "/unused", 4000,
                     1200,      30,
                     160,       0,
                     1920,      1200,
                     1280,      720,
                     0};
  Processor processor(c);
  std::vector<uint8_t> output(1280 * 720 * 2);
  MdK1CameraFrame meta{};
  processor.run(frame, output.data(), output.size(), meta);
  FILE *out = std::fopen(dump, "wbx");
  require(out, "output must be new");
  const auto written = std::fwrite(output.data(), 1, output.size(), out);
  const int rc = std::fclose(out);
  require(written == output.size() && !rc, "dump write failed");
  checked(VDEC_ReleaseFrame(0, frame.ulBufferId), "VDEC_ReleaseFrame");
  checked(VDEC_DisableChn(0), "VDEC_DisableChn");
  checked(VDEC_DestroyChn(0), "VDEC_DestroyChn");
  checked(VDEC_Exit(), "VDEC_Exit");
  checked(VB_Exit(), "VB_Exit");
  checked(SYS_Exit(), "SYS_Exit");
  std::cout << "same-JPEG decode/processor PASS\n";
}

int main(int argc, char **argv) {
  try {
    cv::setNumThreads(1);
    if (argc == 3) {
      saved_jpeg(argv[1], argv[2]);
      return 0;
    }
    require(argc == 1,
            "Usage: camera-image-test [SAVED_4000x1200.jpg NEW.uyvy]");
    for (uint32_t rotation : {0u, 90u, 180u, 270u}) {
      check_image(rotation, false, false);
      check_image(rotation, true, false);
      check_image(rotation, false, true);
      check_image(rotation, true, false, true, false);
      check_image(rotation, true, false, true, true);
    }
    invalid_inputs();
    std::cout << "20 exact image tests PASS\n";
    return 0;
  } catch (const std::exception &e) {
    std::cerr << e.what() << '\n';
    return 1;
  }
}
