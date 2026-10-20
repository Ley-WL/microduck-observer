// Fixed 640x480 NV12 stdin -> corrected hardware MJPEG stdout.
// Observer owns this process; no software encoder or fallback is provided.
#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <cerrno>
#include <utility>
#include <unistd.h>
#include "rk_mpi.h"
#include "mpp_buffer.h"
#include "mpp_frame.h"
#include "mpp_packet.h"
#include "rk_venc_cfg.h"
#include "im2d.h"
#include "vision-frame.h"

constexpr int W=640,H=480,STRIDE_H=480;
constexpr size_t RAW=W*H*3/2;
struct Resources {
    MppCtx ctx=nullptr; MppApi* api=nullptr; MppEncCfg cfg=nullptr;
    MppBufferGroup group=nullptr; MppBuffer raw=nullptr,rgb=nullptr,yuv=nullptr;
    MppFrame frame=nullptr;
    rga_buffer_handle_t a=0,b=0,c=0;
    ~Resources() {
        if(ctx) mpp_destroy(ctx);
        if(frame) mpp_frame_deinit(&frame);
        if(cfg) mpp_enc_cfg_deinit(cfg);
        if(a) releasebuffer_handle(a);
        if(b) releasebuffer_handle(b);
        if(c) releasebuffer_handle(c);
        if(raw) mpp_buffer_put(raw);
        if(rgb) mpp_buffer_put(rgb);
        if(yuv) mpp_buffer_put(yuv);
        if(group) mpp_buffer_group_put(group);
    }
};
#define CHECK(call) do { int rc=(call); if(rc!=MPP_OK) { \
    fprintf(stderr,"%s failed: %d\n",#call,rc);return 1;} } while(0)
static bool write_all(const void* data,size_t size) {
    auto p=static_cast<const unsigned char*>(data);
    while(size) {ssize_t n=write(STDOUT_FILENO,p,size);
        if(n<0 && errno==EINTR) continue;
        if(n<=0) return false;
        p+=n;size-=n;
    }
    return true;
}
int main(int argc,char**) {
    if(argc!=1) {fprintf(stderr,"No arguments: fixed NV12 640x480, 21fps input / 10.5fps output\n");return 2;}
    Resources r;
    CHECK(mpp_create(&r.ctx,&r.api));
    MppPollType timeout=MPP_POLL_BLOCK;
    CHECK(r.api->control(r.ctx,MPP_SET_OUTPUT_TIMEOUT,&timeout));
    CHECK(mpp_init(r.ctx,MPP_CTX_ENC,MPP_VIDEO_CodingMJPEG));
    CHECK(mpp_enc_cfg_init(&r.cfg));
    CHECK(r.api->control(r.ctx,MPP_ENC_GET_CFG,r.cfg));
    for(auto item : {std::pair<const char*,int>{"prep:width",W},
        {"prep:height",H},{"prep:hor_stride",W},{"prep:ver_stride",STRIDE_H},
        {"prep:format",MPP_FMT_YUV420SP},{"rc:mode",MPP_ENC_RC_MODE_FIXQP},
        {"rc:fps_in_num",21},{"rc:fps_in_denorm",2},
        {"rc:fps_out_num",21},{"rc:fps_out_denorm",2},
        {"jpeg:q_factor",80},{"jpeg:qf_max",80},{"jpeg:qf_min",80}}) {
        CHECK(mpp_enc_cfg_set_s32(r.cfg,item.first,item.second));
    }
    CHECK(r.api->control(r.ctx,MPP_ENC_SET_CFG,r.cfg));
    CHECK(mpp_buffer_group_get_internal(&r.group,MPP_BUFFER_TYPE_DMA_HEAP | MPP_BUFFER_FLAGS_CACHABLE));
    CHECK(mpp_buffer_get(r.group,&r.raw,RAW));
    CHECK(mpp_buffer_get(r.group,&r.rgb,W*H*3));
    CHECK(mpp_buffer_get(r.group,&r.yuv,W*STRIDE_H*3/2));
    r.a=importbuffer_fd(mpp_buffer_get_fd(r.raw),RAW);
    r.b=importbuffer_fd(mpp_buffer_get_fd(r.rgb),W*H*3);
    r.c=importbuffer_fd(mpp_buffer_get_fd(r.yuv),W*STRIDE_H*3/2);
    if(!r.a || !r.b || !r.c) {fprintf(stderr,"RGA DMA import failed\n");return 1;}
    auto raw=wrapbuffer_handle(r.a,W,H,RK_FORMAT_YCbCr_420_SP,W,H);
    auto rgb=wrapbuffer_handle(r.b,W,H,RK_FORMAT_RGB_888,W,H);
    auto yuv=wrapbuffer_handle(r.c,W,H,RK_FORMAT_YCbCr_420_SP,W,STRIDE_H);
    CHECK(mpp_frame_init(&r.frame));
    mpp_frame_set_width(r.frame,W);mpp_frame_set_height(r.frame,H);
    mpp_frame_set_hor_stride(r.frame,W);mpp_frame_set_ver_stride(r.frame,STRIDE_H);
    mpp_frame_set_fmt(r.frame,MPP_FMT_YUV420SP);mpp_frame_set_buffer(r.frame,r.yuv);
    std::array<std::array<unsigned char,256>,3> lut{};
    // Neutral-paper center calibration; fixed gains do not correct color shading.
    const double gain[]={1.46,1.0,1.57};
    for(int ch=0;ch<3;ch++) for(int i=0;i<256;i++)
        lut[ch][i]=static_cast<unsigned char>(255*pow(fmin(i*gain[ch]/255,1.0),1/1.30));
    std::array<unsigned char,RAW> input{};
    unsigned frame=0;
    fprintf(stderr,"camera: RGA DMA + MPP JPEG, fixed WB/gamma, no software codec\n");
    for(;;) {
        size_t got=0;
        while(got<RAW) {
            ssize_t n=read(STDIN_FILENO,input.data()+got,RAW-got);
            if(n<0 && errno==EINTR) continue;
            if(n==0 && got==0) return 0;
            if(n<=0) {fprintf(stderr,"Truncated NV12 input/read failure\n");return 1;}
            got+=n;
        }
        if(frame++%2!=0) continue;
        CHECK(mpp_buffer_sync_begin(r.raw));
        memcpy(mpp_buffer_get_ptr(r.raw),input.data(),RAW);
        CHECK(mpp_buffer_sync_end(r.raw));
        IM_STATUS rc=imcvtcolor(raw,rgb,RK_FORMAT_YCbCr_420_SP,RK_FORMAT_RGB_888,IM_YUV_TO_RGB_BT601_FULL);
        if(rc!=IM_STATUS_SUCCESS) {fprintf(stderr,"RGA NV12->RGB: %s\n",imStrError(rc));return 1;}
        CHECK(mpp_buffer_sync_begin(r.rgb));
        auto pixels=static_cast<unsigned char*>(mpp_buffer_get_ptr(r.rgb));
        for(size_t i=0;i<W*H*3;i+=3) {
            pixels[i]=lut[0][pixels[i]];pixels[i+1]=lut[1][pixels[i+1]];
            pixels[i+2]=lut[2][pixels[i+2]];
        }
        if(!vision_publish(pixels,frame)) {fprintf(stderr,"vision latest-frame write failed\n");}
        CHECK(mpp_buffer_sync_end(r.rgb));
        rc=imcvtcolor(rgb,yuv,RK_FORMAT_RGB_888,RK_FORMAT_YCbCr_420_SP,IM_RGB_TO_YUV_BT601_FULL);
        if(rc!=IM_STATUS_SUCCESS) {fprintf(stderr,"RGA RGB->NV12: %s\n",imStrError(rc));return 1;}
        mpp_frame_set_pts(r.frame,static_cast<int64_t>(frame/2)*95238);
        CHECK(r.api->encode_put_frame(r.ctx,r.frame));
        MppPacket packet=nullptr;
        CHECK(r.api->encode_get_packet(r.ctx,&packet));
        if(!packet) {fprintf(stderr,"MPP returned no JPEG\n");return 1;}
        size_t length=mpp_packet_get_length(packet);
        auto bytes=static_cast<unsigned char*>(mpp_packet_get_pos(packet));
        bool valid=length>=4 && length<=1024*1024 && bytes[0]==0xff && bytes[1]==0xd8;
        bool ok=valid && write_all(bytes,length);
        mpp_packet_deinit(&packet);
        if(!ok) {fprintf(stderr,"Invalid MPP JPEG/output write failed\n");return 1;}
    }
}
