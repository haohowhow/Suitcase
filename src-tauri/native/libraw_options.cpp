#include "libraw/libraw.h"

extern "C" void libraw_set_half_size(libraw_data_t *data, int value) {
  if (data) {
    data->params.half_size = value;
  }
}

extern "C" void libraw_set_use_camera_wb(libraw_data_t *data, int value) {
  if (data) {
    data->params.use_camera_wb = value;
  }
}
