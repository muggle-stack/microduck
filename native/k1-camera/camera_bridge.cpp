#include "camera_bridge.h"

// MPP defines MIN/MAX unconditionally; OpenCV guards these definitions. Keep
// the vendor headers before OpenCV without disabling warning checks.
#include <sys_api.h>
#include <uvc_api.h>
#include <v2d_api.h>
#include <vb_api.h>
#include <vdec_api.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <fcntl.h>
#include <linux/dma-buf.h>
#include <linux/dma-heap.h>
#include <memory>
#include <mutex>
#include <opencv2/core.hpp>
#include <opencv2/imgproc.hpp>
#include <stdexcept>
#include <string>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <thread>
#include <unistd.h>

namespace {
using Clock = std::chrono::steady_clock;
std::atomic<bool> in_use{false};
// The pinned MPP revision keeps this code private in mpi/uvc/uvc.c.
constexpr int kUvcTimeout = -11;
void require(bool ok, const std::string &message) {
  if (!ok)
    throw std::runtime_error(message);
}
void checked(int rc, const char *operation) {
  require(rc == 0, std::string(operation) + " failed: " + std::to_string(rc));
}
uint64_t micros(Clock::time_point a, Clock::time_point b) {
  return std::chrono::duration_cast<std::chrono::microseconds>(b - a).count();
}
void error_text(char *dest, size_t size, const char *message) noexcept {
  if (dest && size)
    std::snprintf(dest, size, "%s", message);
}

class Sync {
  int fd_;
  uint64_t flags_;

public:
  Sync(int fd, uint64_t flags) : fd_(fd), flags_(flags) {
    dma_buf_sync s{};
    s.flags = DMA_BUF_SYNC_START | flags_;
    require(ioctl(fd_, DMA_BUF_IOCTL_SYNC, &s) == 0,
            "DMA-BUF sync start failed");
  }
  ~Sync() {
    dma_buf_sync s{};
    s.flags = DMA_BUF_SYNC_END | flags_;
    if (ioctl(fd_, DMA_BUF_IOCTL_SYNC, &s))
      std::fprintf(stderr, "microduck-k1: DMA-BUF sync end failed\n");
  }
  Sync(const Sync &) = delete;
  Sync &operator=(const Sync &) = delete;
};

// One persistent contiguous NV12 DMA allocation. No per-frame heap allocation.
class Image {
  int fd_ = -1;
  void *data_ = MAP_FAILED;
  size_t size_ = 0;

public:
  VideoFrameInfo frame{};
  Image(uint32_t w, uint32_t h) {
    const int heap = open("/dev/dma_heap/system", O_RDWR | O_CLOEXEC);
    require(heap >= 0, "cannot open /dev/dma_heap/system");
    size_ = size_t(w) * h * 3 / 2;
    dma_heap_allocation_data allocation{};
    allocation.len = size_;
    allocation.fd_flags = O_RDWR | O_CLOEXEC;
    const int rc = ioctl(heap, DMA_HEAP_IOCTL_ALLOC, &allocation);
    close(heap);
    require(rc == 0, "DMA heap allocation failed");
    fd_ = int(allocation.fd);
    data_ = mmap(nullptr, size_, PROT_READ | PROT_WRITE, MAP_SHARED, fd_, 0);
    if (data_ == MAP_FAILED) {
      close(fd_);
      fd_ = -1;
      throw std::runtime_error("DMA mmap failed");
    }
    frame.eFrameType = FRAME_TYPE_COMMON;
    frame.eModId = MPP_ID_V2D;
    frame.stCommFrameInfo.u32Width = w;
    frame.stCommFrameInfo.u32Height = h;
    frame.stCommFrameInfo.ePixelFormat = MPP_PIXEL_FORMAT_NV12;
    frame.stCommFrameInfo.eColorSpace = COLOR_SPACE_BT601;
    auto &p = frame.stVFrame;
    p.u32PlaneNum = 2;
    p.u32TotalSize = uint32_t(size_);
    for (int i = 0; i < 2; ++i) {
      p.u32Fd[i] = fd_;
      p.u32PlaneStride[i] = w;
      p.u32PlaneSize[i] = w * h / (i + 1);
      p.u32PlaneSizeValid[i] = p.u32PlaneSize[i];
      p.ulPlaneVirAddr[i] =
          reinterpret_cast<UL>(static_cast<uint8_t *>(data_) + (i ? w * h : 0));
    }
  }
  ~Image() {
    if (data_ != MAP_FAILED)
      munmap(data_, size_);
    if (fd_ >= 0)
      close(fd_);
  }
  Image(const Image &) = delete;
  Image &operator=(const Image &) = delete;
  int fd() const { return fd_; }
  cv::Mat y() const {
    return cv::Mat(frame.stCommFrameInfo.u32Height,
                   frame.stCommFrameInfo.u32Width, CV_8UC1, data_);
  }
  cv::Mat uv() const {
    return cv::Mat(frame.stCommFrameInfo.u32Height / 2,
                   frame.stCommFrameInfo.u32Width / 2, CV_8UC2,
                   reinterpret_cast<void *>(frame.stVFrame.ulPlaneVirAddr[1]));
  }
  void black() {
    Sync sync(fd_, DMA_BUF_SYNC_WRITE);
    y().setTo(0);
    uv().setTo(cv::Scalar(128, 128)); // full-range, converted once when packing
  }
};

struct Job {
  V2DHandle handle = 0;
  Job() { checked(V2D_BeginJob(&handle), "V2D_BeginJob"); }
  ~Job() {
    if (handle)
      V2D_CancelJob(handle);
  }
  void finish() {
    auto h = handle;
    handle = 0;
    checked(V2D_EndJob(h), "V2D_EndJob");
  }
};

struct Geometry {
  V2DArea source{}, destination{};
  uint32_t width, height;
  explicit Geometry(const MdK1CameraConfig &c) {
    require(c.width && c.height && c.width <= 4096 && c.height <= 2160 &&
                !(c.width & 1) && !(c.height & 1),
            "K1 MJPEG dimensions must be even and at most 4096x2160");
    require(c.output_width && c.output_height && c.output_width <= 4096 &&
                c.output_height <= 2160 && !(c.output_width & 1) &&
                !(c.output_height & 1),
            "invalid K1 output dimensions");
    require(c.fps >= 1 && c.fps <= 120, "invalid native frame rate");
    require(c.rotation == 0 || c.rotation == 90 || c.rotation == 180 ||
                c.rotation == 270,
            "invalid rotation");
    require(c.crop_width && c.crop_height && c.crop_x <= c.width &&
                c.crop_y <= c.height && c.crop_width <= c.width - c.crop_x &&
                c.crop_height <= c.height - c.crop_y &&
                !((c.crop_x | c.crop_y | c.crop_width | c.crop_height) & 1),
            "K1 NV12 ROI needs even, in-bounds coordinates and dimensions");
    const double scale = std::min(double(c.output_width) / c.crop_width,
                                  double(c.output_height) / c.crop_height);
    const auto fw = uint32_t(std::lround(c.crop_width * scale)),
               fh = uint32_t(std::lround(c.crop_height * scale));
    const auto x = (c.output_width - fw) / 2, y = (c.output_height - fh) / 2;
    require(fw && fh && !((fw | fh | x | y) & 1),
            "K1 letterbox geometry is not NV12-aligned; use software or "
            "aligned output geometry");
    require(uint64_t(fw) * 8 >= c.crop_width &&
                uint64_t(fh) * 8 >= c.crop_height &&
                fw <= uint64_t(c.crop_width) * 8 &&
                fh <= uint64_t(c.crop_height) * 8,
            "V2D supports only 1/8 to 8x per-axis scaling");
    source = {uint16_t(c.crop_x), uint16_t(c.crop_y), uint16_t(c.crop_width),
              uint16_t(c.crop_height)};
    destination = {uint16_t(x), uint16_t(y), uint16_t(fw), uint16_t(fh)};
    width = c.rotation == 90 || c.rotation == 270 ? c.output_height
                                                  : c.output_width;
    height = c.rotation == 90 || c.rotation == 270 ? c.output_width
                                                   : c.output_height;
  }
};

class Processor {
  MdK1CameraConfig config_;
  Geometry geometry_;
  std::unique_ptr<Image> normalized_, scaled_, rotated_;
  cv::Mat y_limited_, uv_limited_, uv_full_, y_lut_, uv_lut_;

public:
  explicit Processor(const MdK1CameraConfig &c) : config_(c), geometry_(c) {
    scaled_ = std::make_unique<Image>(c.output_width, c.output_height);
    scaled_->black();
    if (c.rotation)
      rotated_ = std::make_unique<Image>(geometry_.width, geometry_.height);
    y_lut_ = cv::Mat(1, 256, CV_8UC1);
    uv_lut_ = cv::Mat(1, 256, CV_8UC1);
    for (int i = 0; i < 256; ++i) {
      y_lut_.at<uint8_t>(i) = uint8_t(std::lround(16.0 + i * 219.0 / 255.0));
      uv_lut_.at<uint8_t>(i) =
          uint8_t(std::lround(128.0 + (i - 128) * 224.0 / 255.0));
    }
  }
  const Geometry &geometry() const { return geometry_; }
  void run(const VideoFrameInfo &input, uint8_t *output, size_t capacity,
           MdK1CameraFrame &meta) {
    require(capacity >= size_t(geometry_.width) * geometry_.height * 2,
            "UYVY output buffer is too small");
    require(input.stCommFrameInfo.ePixelFormat == MPP_PIXEL_FORMAT_NV12 &&
                input.stCommFrameInfo.u32Width == config_.width &&
                input.stCommFrameInfo.u32Height == config_.height,
            "decoder returned an unexpected format/resolution");
    auto start = Clock::now();
    const auto &p = input.stVFrame;
    require(p.u32PlaneNum >= 2 && p.u32Fd[0] > 0 &&
                p.u32PlaneStride[0] >= config_.width &&
                p.u32PlaneStride[1] >= config_.width && p.ulPlaneVirAddr[0] &&
                p.ulPlaneVirAddr[1],
            "invalid decoder NV12 planes");
    require(p.u32PlaneSize[0] >=
                    uint64_t(p.u32PlaneStride[0]) * config_.height &&
                p.u32PlaneSize[1] >=
                    uint64_t(p.u32PlaneStride[1]) * config_.height / 2,
            "short decoder NV12 allocation");
    const VideoFrameInfo *source = &input;
    // V2D takes ONE fd plus a UV offset. Never interpret split-plane DMA as
    // contiguous.
    if (p.u32Fd[0] != p.u32Fd[1] ||
        p.u32PlaneStride[0] != p.u32PlaneStride[1] ||
        p.ulPlaneVirAddr[1] != p.ulPlaneVirAddr[0] + p.u32PlaneSize[0]) {
      if (!normalized_)
        normalized_ = std::make_unique<Image>(config_.width, config_.height);
      Sync src_y(int(p.u32Fd[0]), DMA_BUF_SYNC_READ);
      std::unique_ptr<Sync> src_uv;
      if (p.u32Fd[0] != p.u32Fd[1])
        src_uv = std::make_unique<Sync>(int(p.u32Fd[1]), DMA_BUF_SYNC_READ);
      Sync dst(normalized_->fd(), DMA_BUF_SYNC_WRITE);
      cv::Mat(config_.height, config_.width, CV_8UC1,
              reinterpret_cast<void *>(p.ulPlaneVirAddr[0]),
              p.u32PlaneStride[0])
          .copyTo(normalized_->y());
      cv::Mat(config_.height / 2, config_.width / 2, CV_8UC2,
              reinterpret_cast<void *>(p.ulPlaneVirAddr[1]),
              p.u32PlaneStride[1])
          .copyTo(normalized_->uv());
      source = &normalized_->frame;
    }
    Job job;
    checked(V2D_AddBitblitTask(job.handle, source, &geometry_.source,
                               &scaled_->frame, &geometry_.destination,
                               V2D_CSC_MODE_BUTT),
            "V2D crop/resize");
    if (rotated_)
      checked(V2D_RotateFrame(job.handle, &scaled_->frame, &rotated_->frame,
                              config_.rotation == 90    ? V2D_ROT_90
                              : config_.rotation == 180 ? V2D_ROT_180
                                                        : V2D_ROT_270),
              "V2D rotate");
    job.finish();
    auto image_done = Clock::now();
    Image &image = rotated_ ? *rotated_ : *scaled_;
    Sync read(image.fd(), DMA_BUF_SYNC_READ);
    // JPEG is full-range BT.601. Preserve YUV values through V2D, then map
    // range and repack to the existing LIMITED-range UYVY contract without RGB
    // round trips.
    cv::LUT(image.y(), y_lut_, y_limited_);
    cv::LUT(image.uv(), uv_lut_, uv_limited_);
    cv::resize(uv_limited_, uv_full_,
               cv::Size(geometry_.width / 2, geometry_.height), 0, 0,
               cv::INTER_NEAREST);
    cv::Mat channels[] = {uv_full_, y_limited_.reshape(2)};
    cv::Mat packed(geometry_.height, geometry_.width / 2, CV_8UC4, output);
    const int mapping[] = {0, 0, 2, 1, 1, 2, 3, 3}; // U,V,Y0,Y1 -> U,Y0,V,Y1
    cv::mixChannels(channels, 2, &packed, 1, mapping, 4);
    meta.image_us = micros(start, image_done);
    meta.pack_us = micros(image_done, Clock::now());
    meta.width = geometry_.width;
    meta.height = geometry_.height;
    meta.bytes = meta.width * meta.height * 2;
  }
};

class Camera {
  MdK1CameraConfig config_;
  std::unique_ptr<Processor> processor_;
  std::thread feeder_;
  std::atomic<bool> stop_{false};
  std::mutex error_mutex_;
  std::string feeder_error_;
  bool sys_ = false, vb_ = false, uvc_ = false, vdec_ = false, device_ = false,
       device_on_ = false, channel_on_ = false, decoder_ = false,
       decoder_on_ = false;
  uint64_t first_pts_ = 0, last_pts_ = 0, sequence_ = 0;
  void feed() noexcept {
    try {
      while (!stop_) {
        VideoFrameInfo frame{};
        int rc = UVC_GetFrame(0, 0, &frame, 200);
        if (rc == kUvcTimeout)
          continue;
        checked(rc, "UVC_GetFrame");
        struct ReleaseUvc {
          VideoFrameInfo *frame;
          ~ReleaseUvc() { UVC_ReleaseFrame(0, 0, frame); }
        } release{&frame};
        StreamBufferInfo stream{};
        stream.pu8Addr =
            reinterpret_cast<const U8 *>(frame.stVFrame.ulPlaneVirAddr[0]);
        stream.u32Size = frame.stVFrame.u32PlaneSizeValid[0];
        stream.u64PTS = frame.stVFrame.u64PTS;
        stream.eCodecType = MPP_STREAM_CODEC_MJPEG;
        stream.bKeyFrame = MPP_TRUE;
        stream.u32Width = config_.width;
        stream.u32Height = config_.height;
        {
          Sync cpu_read(int(frame.stVFrame.u32Fd[0]), DMA_BUF_SYNC_READ);
          // The pinned decoder copies compressed bytes into its input buffer
          // before returning. The UVC reference survives that copy and sync.
          rc = VDEC_SendStream(0, &stream, 200);
        }
        if (rc != ERR_VDEC_TIMEOUT && rc != ERR_VDEC_BUSY)
          checked(rc, "VDEC_SendStream");
        // Timed-out compressed frames are dropped, not queued without bound.
      }
    } catch (const std::exception &e) {
      std::lock_guard<std::mutex> lock(error_mutex_);
      feeder_error_ = e.what();
    }
  }

public:
  explicit Camera(const MdK1CameraConfig &c) : config_(c) {}
  void start() {
    require(config_.device && config_.device[0] == '/' &&
                std::strlen(config_.device) < 128,
            "explicit camera device path required (max 127 bytes)");
    Geometry geometry(config_);
    (void)geometry;
    require(cv::getCPUFeaturesLine().find("RVV") != std::string::npos,
            "SpaceMIT OpenCV with RVV support is required");
    cv::setNumThreads(1);
    checked(SYS_Init(), "SYS_Init");
    sys_ = true;
    checked(VB_Init(), "VB_Init");
    vb_ = true;
    checked(UVC_Init(), "UVC_Init");
    uvc_ = true;
    checked(VDEC_Init(), "VDEC_Init");
    vdec_ = true;
    UvcDevAttr device{};
    std::snprintf(device.acDevNode, sizeof(device.acDevNode), "%s",
                  config_.device);
    checked(UVC_CreateDev(0, &device), "UVC_CreateDev");
    device_ = true;
    checked(UVC_EnableDev(0), "UVC_EnableDev");
    device_on_ = true;
    UvcChnAttr channel{};
    channel.u32Width = config_.width;
    channel.u32Height = config_.height;
    channel.u32Fps = config_.fps;
    channel.u32Depth = 2;
    channel.ePixelFormat = MPP_PIXEL_FORMAT_MJPEG;
    checked(UVC_SetChnAttr(0, 0, &channel), "UVC_SetChnAttr");
    checked(UVC_EnableChn(0, 0), "UVC_EnableChn");
    channel_on_ = true;
    UvcChnAttr negotiated{};
    checked(UVC_GetChnAttr(0, 0, &negotiated), "UVC_GetChnAttr");
    require(negotiated.u32Width == config_.width &&
                negotiated.u32Height == config_.height &&
                negotiated.u32Fps == config_.fps &&
                negotiated.ePixelFormat == MPP_PIXEL_FORMAT_MJPEG,
            "camera changed the requested native mode; refusing an implicit "
            "fallback");
    VdecChnAttr decoder{};
    decoder.eCodecType = MPP_STREAM_CODEC_MJPEG;
    decoder.eOutputPixelFormat = MPP_PIXEL_FORMAT_NV12;
    decoder.u32Width = config_.width;
    decoder.u32Height = config_.height;
    decoder.u32BufCnt = 6;
    checked(VDEC_CreateChn(0, &decoder), "VDEC_CreateChn");
    decoder_ = true;
    checked(VDEC_EnableChn(0), "VDEC_EnableChn");
    decoder_on_ = true;
    processor_ = std::make_unique<Processor>(config_);
    feeder_ = std::thread(&Camera::feed, this);
  }
  ~Camera() {
    stop_ = true;
    if (feeder_.joinable())
      feeder_.join();
    if (channel_on_)
      UVC_DisableChn(0, 0);
    if (decoder_on_)
      VDEC_DisableChn(0);
    processor_.reset();
    if (decoder_)
      VDEC_DestroyChn(0);
    if (device_on_)
      UVC_DisableDev(0);
    if (device_)
      UVC_DestroyDev(0);
    if (vdec_)
      VDEC_Exit();
    if (uvc_)
      UVC_Exit();
    if (vb_)
      VB_Exit();
    if (sys_)
      SYS_Exit();
  }
  void read(uint8_t *out, size_t capacity, uint32_t timeout_ms,
            MdK1CameraFrame &meta) {
    require(out && timeout_ms >= 1 && timeout_ms <= 30000,
            "invalid read buffer/timeout");
    const auto start = Clock::now(),
               deadline = start + std::chrono::milliseconds(timeout_ms);
    VideoFrameInfo frame{};
    while (true) {
      {
        std::lock_guard<std::mutex> lock(error_mutex_);
        require(feeder_error_.empty(), feeder_error_);
      }
      require(Clock::now() < deadline,
              "K1 camera/decoder produced no frame before timeout");
      const auto remaining =
          std::chrono::duration_cast<std::chrono::milliseconds>(deadline -
                                                                Clock::now())
              .count();
      int rc = VDEC_GetLatestFrame(
          0, &frame, uint32_t(std::clamp<int64_t>(remaining, 1, 100)));
      if (rc == ERR_VDEC_TIMEOUT || rc == ERR_VDEC_NO_FRAME)
        continue;
      checked(rc, "VDEC_GetLatestFrame");
      break;
    }
    // This guard runs even if format validation, V2D, or OpenCV throws.
    struct Release {
      UL id;
      ~Release() { VDEC_ReleaseFrame(0, id); }
    } release{frame.ulBufferId};
    meta = {};
    meta.wait_us = micros(start, Clock::now());
    const uint64_t pts = frame.stVFrame.u64PTS;
    require(pts && (!last_pts_ || pts > last_pts_),
            "non-increasing/missing capture PTS from MPP");
    if (!first_pts_)
      first_pts_ = pts;
    last_pts_ = pts;
    meta.pts_ns = (pts - first_pts_) * 1000;
    meta.sequence = sequence_++;
    processor_->run(frame, out, capacity, meta);
  }
};
} // namespace

extern "C" {
uint32_t md_k1_camera_abi() { return MD_K1_CAMERA_ABI; }
const char *md_k1_camera_build_info() {
  return "MPP 2b97ffe (isolated codec2/V2D); OpenCV " CV_VERSION;
}
int md_k1_camera_open(const MdK1CameraConfig *config, void **handle,
                      char *error, size_t error_size) {
  if (handle)
    *handle = nullptr;
  if (!config || !handle || config->struct_size != sizeof(*config) ||
      config->abi != MD_K1_CAMERA_ABI) {
    error_text(error, error_size, "K1 camera ABI/config mismatch");
    return -1;
  }
  bool expected = false;
  if (!in_use.compare_exchange_strong(expected, true)) {
    error_text(error, error_size,
               "only one K1 MPP camera context is supported per process");
    return -1;
  }
  try {
    auto camera = std::make_unique<Camera>(*config);
    camera->start();
    *handle = camera.release();
    error_text(error, error_size, "");
    return 0;
  } catch (const std::exception &e) {
    error_text(error, error_size, e.what());
  } catch (...) {
    error_text(error, error_size, "unknown K1 camera initialization failure");
  }
  in_use = false;
  return -1;
}
int md_k1_camera_read(void *handle, uint8_t *uyvy, size_t capacity,
                      uint32_t timeout_ms, MdK1CameraFrame *frame, char *error,
                      size_t error_size) {
  if (!handle || !frame) {
    error_text(error, error_size, "null K1 camera handle/frame");
    return -1;
  }
  try {
    static_cast<Camera *>(handle)->read(uyvy, capacity, timeout_ms, *frame);
    error_text(error, error_size, "");
    return 0;
  } catch (const std::exception &e) {
    error_text(error, error_size, e.what());
  } catch (...) {
    error_text(error, error_size, "unknown K1 camera read failure");
  }
  return -1;
}
void md_k1_camera_close(void *handle) {
  if (handle) {
    delete static_cast<Camera *>(handle);
    in_use = false;
  }
}
}

#ifdef MICRODUCK_IMAGE_TEST
#include "image_tests.inc"
#endif
