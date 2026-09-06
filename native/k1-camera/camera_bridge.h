// Optional K1-only ABI. No MPP/OpenCV types cross into the portable Rust SDK.
#ifndef MICRODUCK_K1_CAMERA_H
#define MICRODUCK_K1_CAMERA_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MD_K1_CAMERA_ABI 1u
typedef struct {
  uint32_t struct_size, abi;
  const char *device;
  uint32_t width, height, fps;
  uint32_t crop_x, crop_y, crop_width, crop_height;
  uint32_t output_width, output_height;
  uint32_t rotation; // physical clockwise turn AFTER crop/letterbox; normally 0
} MdK1CameraConfig;
typedef struct {
  uint64_t pts_ns, sequence;
  uint64_t wait_us, image_us, pack_us;
  uint32_t width, height, bytes;
} MdK1CameraFrame;
uint32_t md_k1_camera_abi(void);
const char *md_k1_camera_build_info(void);
// All calls catch C++ exceptions. One camera context per process; read is
// serialized. Return 0 on success, -1 on failure. Error text is always bounded
// and NUL terminated.
int md_k1_camera_open(const MdK1CameraConfig *, void **handle, char *error,
                      size_t error_size);
int md_k1_camera_read(void *handle, uint8_t *uyvy, size_t capacity,
                      uint32_t timeout_ms, MdK1CameraFrame *frame, char *error,
                      size_t error_size);
void md_k1_camera_close(void *handle);
#ifdef __cplusplus
}
#endif
#endif
