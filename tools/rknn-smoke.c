// 独立 RKNN 冒烟测试：固定 224x224 RGB 输入，不连接机器人控制接口。
#define _POSIX_C_SOURCE 200809L
#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_JPEG
#include "stb_image.h"
#include "rknn_api.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
  struct timespec t;
  clock_gettime(CLOCK_MONOTONIC, &t);
  return t.tv_sec * 1000.0 + t.tv_nsec / 1e6;
}

#define CHECK(call) do { int rc = (call); if (rc != RKNN_SUCC) { \
  fprintf(stderr, "%s failed: %d\n", #call, rc); goto cleanup; \
} } while (0)

int main(int argc, char **argv) {
  int result = 1, width = 0, height = 0, channels = 0;
  rknn_context ctx = 0;
  unsigned char *image = NULL;
  void *model = NULL;
  FILE *file = NULL;
  rknn_output output = {0};
  int output_live = 0;
  if (argc != 3) {
    fprintf(stderr, "Usage: %s model.rknn image_224x224.jpg\n", argv[0]);
    return 2;
  }
  image = stbi_load(argv[2], &width, &height, &channels, 3);
  if (!image || width != 224 || height != 224) {
    fprintf(stderr, "Expected a valid 224x224 JPEG\n");
    goto cleanup;
  }
  file = fopen(argv[1], "rb");
  if (!file || fseek(file, 0, SEEK_END)) goto cleanup;
  long size = ftell(file);
  if (size <= 0 || size > 64 * 1024 * 1024) goto cleanup;
  rewind(file);
  model = malloc((size_t)size);
  if (!model || fread(model, 1, (size_t)size, file) != (size_t)size)
    goto cleanup;
  fclose(file);
  file = NULL;
  CHECK(rknn_init(&ctx, model, (uint32_t)size, 0, NULL));
  rknn_sdk_version version = {0};
  CHECK(rknn_query(ctx, RKNN_QUERY_SDK_VERSION, &version, sizeof(version)));
  printf("runtime=%s driver=%s\n", version.api_version, version.drv_version);
  rknn_input_output_num counts = {0};
  CHECK(rknn_query(ctx, RKNN_QUERY_IN_OUT_NUM, &counts, sizeof(counts)));
  if (counts.n_input != 1 || counts.n_output != 1) goto cleanup;
  rknn_tensor_attr input_attr = {0}, output_attr = {0};
  CHECK(rknn_query(ctx, RKNN_QUERY_INPUT_ATTR, &input_attr, sizeof(input_attr)));
  CHECK(rknn_query(ctx, RKNN_QUERY_OUTPUT_ATTR, &output_attr, sizeof(output_attr)));
  printf("input=%u,%u,%u,%u format=%s output_elements=%u\n",
         input_attr.dims[0], input_attr.dims[1], input_attr.dims[2],
         input_attr.dims[3], get_format_string(input_attr.fmt), output_attr.n_elems);
  if (input_attr.n_elems != 224 * 224 * 3 || output_attr.n_elems != 1001)
    goto cleanup;
  rknn_input input = {0};
  input.buf = image;
  input.size = 224 * 224 * 3;
  input.type = RKNN_TENSOR_UINT8;
  input.fmt = RKNN_TENSOR_NHWC;
  double total = 0, minimum = 1e30, maximum = 0;
  for (int iteration = 0; iteration < 23; ++iteration) {
    double start = now_ms();
    CHECK(rknn_inputs_set(ctx, 1, &input));
    CHECK(rknn_run(ctx, NULL));
    output.want_float = 1;
    CHECK(rknn_outputs_get(ctx, 1, &output, NULL));
    output_live = 1;
    double elapsed = now_ms() - start;
    if (iteration >= 3) {
      total += elapsed;
      if (elapsed < minimum) minimum = elapsed;
      if (elapsed > maximum) maximum = elapsed;
    }
    if (!output.buf || output.size != 1001 * sizeof(float)) goto cleanup;
    float *scores = output.buf;
    for (int i = 0; i < 1001; ++i) if (!isfinite(scores[i])) goto cleanup;
    if (iteration == 22) {
      int selected[5] = {-1, -1, -1, -1, -1};
      for (int rank = 0; rank < 5; ++rank) {
        float best = -INFINITY;
        for (int i = 0; i < 1001; ++i) {
          int used = 0;
          for (int j = 0; j < rank; ++j) used |= selected[j] == i;
          if (!used && scores[i] > best) { best = scores[i]; selected[rank] = i; }
        }
        printf("top%d index=%d score=%.6f\n", rank + 1, selected[rank], best);
      }
    }
    CHECK(rknn_outputs_release(ctx, 1, &output));
    output_live = 0;
  }
  printf("PASS runs=23 warmup=3 measured=20 mean_ms=%.3f min_ms=%.3f max_ms=%.3f\n",
         total / 20, minimum, maximum);
  result = 0;
cleanup:
  if (output_live) rknn_outputs_release(ctx, 1, &output);
  if (ctx) rknn_destroy(ctx);
  if (file) fclose(file);
  free(model);
  stbi_image_free(image);
  if (result) fprintf(stderr, "RKNN smoke test failed\n");
  return result;
}
