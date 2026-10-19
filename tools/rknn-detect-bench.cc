// Native benchmark harness for Rockchip rknn_model_zoo v2.3.2 examples.
// Build once with -DBENCH_YOLO, once without; see vision benchmark record.
#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <vector>
#include <sys/resource.h>
#include <time.h>
#include "image_utils.h"
#include "image_drawing.h"
#ifdef BENCH_YOLO
#include "yolov5.h"
#define MODEL_INIT init_yolov5_model
#define MODEL_RUN inference_yolov5_model
#define MODEL_RELEASE release_yolov5_model
using Result = object_detect_result_list;
#else
#include "retinaface.h"
#define MODEL_INIT init_retinaface_model
#define MODEL_RUN inference_retinaface_model
#define MODEL_RELEASE release_retinaface_model
using Result = retinaface_result;
#endif
using Clock = std::chrono::steady_clock;
static double process_ms() {
    timespec ts{};
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts);
    return ts.tv_sec*1000.0+ts.tv_nsec/1e6;
}
static double run_cpu, resize_cpu, input_cpu, output_cpu;
static double run_ms;
static double resize_ms, input_ms, output_ms;
extern "C" int __real_rknn_init(rknn_context*,void*,uint32_t,uint32_t,rknn_init_extend*);
extern "C" int __wrap_rknn_init(rknn_context* ctx,void* model,uint32_t size,uint32_t flags,rknn_init_extend* ext) {
    if(std::getenv("RKNN_BENCH_PERF")) flags |= RKNN_FLAG_COLLECT_PERF_MASK;
    return __real_rknn_init(ctx,model,size,flags,ext);
}
extern "C" int __real_convert_image_with_letterbox(image_buffer_t*, image_buffer_t*, letterbox_t*, char);
extern "C" int __wrap_convert_image_with_letterbox(image_buffer_t* src, image_buffer_t* dst, letterbox_t* box, char color) {
    auto t=Clock::now();double c=process_ms();
    int rc=__real_convert_image_with_letterbox(src,dst,box,color);
    resize_cpu=process_ms()-c;
    resize_ms=std::chrono::duration<double,std::milli>(Clock::now()-t).count();
    return rc;
}
extern "C" int __real_rknn_inputs_set(rknn_context, uint32_t, rknn_input*);
extern "C" int __wrap_rknn_inputs_set(rknn_context ctx,uint32_t n,rknn_input* p) {
    auto t=Clock::now();double c=process_ms(); int rc=__real_rknn_inputs_set(ctx,n,p);
    input_cpu=process_ms()-c;
    input_ms=std::chrono::duration<double,std::milli>(Clock::now()-t).count();return rc;
}
extern "C" int __real_rknn_outputs_get(rknn_context,uint32_t,rknn_output*,rknn_output_extend*);
extern "C" int __wrap_rknn_outputs_get(rknn_context ctx,uint32_t n,rknn_output* p,rknn_output_extend* ext) {
    auto t=Clock::now();double c=process_ms();int rc=__real_rknn_outputs_get(ctx,n,p,ext);
    output_cpu=process_ms()-c;
    output_ms=std::chrono::duration<double,std::milli>(Clock::now()-t).count();return rc;
}
extern "C" int __real_rknn_run(rknn_context, rknn_run_extend*);
extern "C" int __wrap_rknn_run(rknn_context ctx, rknn_run_extend* ext) {
    const auto t = Clock::now();double c=process_ms();
    const int rc = __real_rknn_run(ctx, ext);
    run_cpu=process_ms()-c;
    run_ms = std::chrono::duration<double, std::milli>(Clock::now()-t).count();
    return rc;
}
static double cpu(const rusage& r) {
    return r.ru_utime.tv_sec + r.ru_utime.tv_usec/1e6 +
           r.ru_stime.tv_sec + r.ru_stime.tv_usec/1e6;
}
static void stats(const char* name, std::vector<double> v) {
    std::sort(v.begin(), v.end());
    double sum=0; for(double x:v) sum+=x;
    printf("BENCH %s mean_ms=%.3f p50_ms=%.3f p95_ms=%.3f min_ms=%.3f max_ms=%.3f\n",
        name, sum/v.size(), v[v.size()/2], v[(v.size()*95-1)/100], v.front(), v.back());
}
int main(int argc, char** argv) {
    if(argc!=5) {fprintf(stderr,"usage: %s model image annotated-output runs(1..500)\n",argv[0]);return 2;}
    char* end=nullptr; long n=strtol(argv[4],&end,10);
    if(!*argv[4] || *end || n<1 || n>500) return 2;
    rknn_app_context_t ctx{}; image_buffer_t img{}; Result result{};
#ifdef BENCH_YOLO
    if(init_post_process()!=0) return 1;
#endif
    if(MODEL_INIT(argv[1],&ctx)!=0) return 1;
    if(read_image(argv[2],&img)!=0) {MODEL_RELEASE(&ctx);return 1;}
    std::vector<double> wall, core, resize, inputs, outputs, rest;
    std::vector<double> cr,ci,co,cn,ct;
    rusage before{},after{}; Clock::time_point start;
    int status=0;
    for(int i=-5;i<n;i++) {
        if(i==0) {getrusage(RUSAGE_SELF,&before); start=Clock::now();}
        auto t=Clock::now();double c=process_ms();
        if(MODEL_RUN(&ctx,&img,&result)!=0) {status=1;break;}
        if(i>=0) {
            double used=process_ms()-c;
            cr.push_back(resize_cpu);ci.push_back(input_cpu);co.push_back(output_cpu);cn.push_back(run_cpu);ct.push_back(used-resize_cpu-input_cpu-output_cpu-run_cpu);
            double total=std::chrono::duration<double,std::milli>(Clock::now()-t).count();
            wall.push_back(total);core.push_back(run_ms);resize.push_back(resize_ms);
            inputs.push_back(input_ms);outputs.push_back(output_ms);
            rest.push_back(total-run_ms-resize_ms-input_ms-output_ms);
        }
    }
    if(!status) {
        double seconds=std::chrono::duration<double>(Clock::now()-start).count();
        getrusage(RUSAGE_SELF,&after);
        stats("pipeline",wall); stats("rknn_run",core);
        stats("resize",resize); stats("inputs_set",inputs); stats("outputs_get",outputs);
        stats("postprocess_and_other",rest);
        stats("cpu_resize",cr);stats("cpu_inputs",ci);stats("cpu_run",cn);stats("cpu_outputs",co);stats("cpu_other",ct);
        printf("BENCH runs=%ld warmup=5 cpu_one_core_pct=%.2f peak_rss_kib=%ld input=%dx%d\n",
            n,100*(cpu(after)-cpu(before))/seconds,after.ru_maxrss,img.width,img.height);
        for(int i=0;i<result.count;i++) {
#ifdef BENCH_YOLO
            const auto& d=result.results[i]; if(d.cls_id!=0) continue;
            const auto& b=d.box; float score=d.prop;
#else
            const auto& d=result.object[i]; const auto& b=d.box; float score=d.score;
#endif
            printf("DETECTION box=%d,%d,%d,%d score=%.6f\n",b.left,b.top,b.right,b.bottom,score);
            draw_rectangle(&img,b.left,b.top,b.right-b.left,b.bottom-b.top,COLOR_GREEN,2);
        }
        // With DISABLE_LIBJPEG, STB returns 1 on successful PNG/JPEG writes.
        if(write_image(argv[3],&img)!=1) status=1;
    }
    if(std::getenv("RKNN_BENCH_PERF") && !status) {
        rknn_perf_detail detail{};
        int rc=rknn_query(ctx.rknn_ctx,RKNN_QUERY_PERF_DETAIL,&detail,sizeof(detail));
        if(rc==0 && detail.perf_data) printf("PERF_DETAIL\n%s\n",detail.perf_data);
        else {fprintf(stderr,"perf query failed: %d\n",rc);status=1;}
    }
    free(img.virt_addr);
    if(MODEL_RELEASE(&ctx)!=0) status=1;
#ifdef BENCH_YOLO
    deinit_post_process();
#endif
    return status;
}
