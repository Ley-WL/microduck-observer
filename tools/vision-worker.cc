// Request-driven RKNN worker: latest RGB -> RGA clockwise rotation/letterbox -> NPU.
// stdout carries bounded JSON only; vendor diagnostics are redirected to stderr.
#include <algorithm>
#include <array>
#include <cerrno>
#include <cstdio>
#include <cstring>
#include <sys/resource.h>
#include "vision-frame.h"
#include "vision-model.h"
#include "image_utils.h"
#include "im2d.h"

static double cpu_ms(){timespec t{};clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&t);return t.tv_sec*1000.+t.tv_nsec/1e6;}
static double resize_ms=0;
extern "C" int get_image_size(image_buffer_t* img){return img->width*img->height*3;}
// Pinned Zoo letterbox geometry, with strict RGA execution and no CPU resize fallback.
extern "C" int convert_image_with_letterbox(image_buffer_t* src,image_buffer_t* dst,letterbox_t* box,char color){
    auto start=vision_ns();
    if(src->format!=IMAGE_FORMAT_RGB888 || dst->format!=IMAGE_FORMAT_RGB888 || !src->virt_addr || !dst->virt_addr)return -1;
    float sw=float(dst->width)/src->width,sh=float(dst->height)/src->height;
    float scale=std::min(sw,sh);int w=dst->width,h=dst->height;
    if(sw<sh)h=int(src->height*scale);else w=int(src->width*scale);
    w-=w%4;h-=h%2;int x=sw<sh?0:(dst->width-w)/2,y=sw<sh?(dst->height-h)/2:0;x-=x%2;y-=y%2;
    auto a=wrapbuffer_virtualaddr(src->virt_addr,src->width,src->height,RK_FORMAT_RGB_888);
    auto b=wrapbuffer_virtualaddr(dst->virt_addr,dst->width,dst->height,RK_FORMAT_RGB_888);
    unsigned c=static_cast<unsigned char>(color);
    IM_STATUS rc=imfill(b,{0,0,dst->width,dst->height},int(0xff000000U|c<<16|c<<8|c));
    if(rc==IM_STATUS_SUCCESS)rc=improcess(a,b,{}, {0,0,src->width,src->height},{x,y,w,h},{},IM_SYNC);
    resize_ms+=(vision_ns()-start)/1e6;
    if(rc!=IM_STATUS_SUCCESS){fprintf(stderr,"vision RGA letterbox: %s\n",imStrError(rc));return -1;}
    *box={x,y,scale};return 0;
}
int main(int argc,char** argv){
    if(argc!=3){fprintf(stderr,"usage: vision-worker person.rknn face.rknn\n");return 2;}
    FILE* output=fdopen(dup(STDOUT_FILENO),"w");if(!output || dup2(STDERR_FILENO,STDOUT_FILENO)<0)return 1;
    setvbuf(output,nullptr,_IOLBF,0);
    if(person_init(argv[1])!=0 || face_init(argv[2])!=0){fprintf(stderr,"vision model initialization failed\n");return 1;}
    int fd=open(vision_path(),O_RDWR|O_CREAT|O_CLOEXEC|O_NOFOLLOW,0600);
    if(fd<0 || ftruncate(fd,sizeof(VisionHeader)+VISION_BYTES)!=0){perror("vision frame");return 1;}
    VisionHeader empty;empty.magic=0;
    if(flock(fd,LOCK_EX)!=0 || !vision_io(fd,&empty,sizeof(empty),0,true))return 1;
    flock(fd,LOCK_UN);
    std::array<unsigned char,VISION_BYTES> raw{},upright{};
    auto a=wrapbuffer_virtualaddr(raw.data(),640,480,RK_FORMAT_RGB_888);
    auto b=wrapbuffer_virtualaddr(upright.data(),480,640,RK_FORMAT_RGB_888);
    image_buffer_t img{};img.width=480;img.height=640;img.format=IMAGE_FORMAT_RGB888;img.size=VISION_BYTES;img.virt_addr=upright.data();
    uint64_t last=0;int code=0;
    for(;;){char request=0;ssize_t n=read(STDIN_FILENO,&request,1);if(n==0)break;if(n<0&&errno==EINTR)continue;
        if(n!=1 || (request!='b'&&request!='p')){fprintf(stderr,"vision: expected p (person) or b (person then face) request\n");code=1;break;}
        auto start=vision_ns();double c=cpu_ms();VisionHeader h;bool read_ok=false;
        if(flock(fd,LOCK_SH|LOCK_NB)==0){read_ok=vision_io(fd,&h,sizeof(h),0,false)&&vision_io(fd,raw.data(),raw.size(),sizeof(h),false);flock(fd,LOCK_UN);}
        if(!read_ok || h.magic!=VisionHeader{}.magic || h.size!=VISION_BYTES || h.sequence==last || h.ns>start || start-h.ns>500000000ULL){fprintf(output,"{\"waiting\":true}\n");continue;}
        last=h.sequence;resize_ms=0;auto rotate_at=vision_ns();
        auto rc=imrotate(a,b,IM_HAL_TRANSFORM_ROT_90);
        if(rc!=IM_STATUS_SUCCESS){fprintf(stderr,"vision RGA rotation: %s\n",imStrError(rc));code=1;break;}
        double rotate=(vision_ns()-rotate_at)/1e6;std::array<VisionBox,64> boxes{};
        // Every request starts with people; never run an independent face search.
        auto person_at=vision_ns();int people=person_run(&img,boxes.data(),32);
        if(people<0){code=1;break;}double person=(vision_ns()-person_at)/1e6;
        int faces=0;double face=0;
        if(request=='b' && people>0){auto face_at=vision_ns();faces=face_run(&img,boxes.data()+people,32);face=(vision_ns()-face_at)/1e6;}
        if(faces<0){code=1;break;}
        rusage usage{};getrusage(RUSAGE_SELF,&usage);
        fprintf(output,"{\"sequence\":%llu,\"capturedNs\":%llu,\"ageMs\":%.3f,\"totalMs\":%.3f,\"cpuMs\":%.3f,\"rotationMs\":%.3f,\"resizeMs\":%.3f,\"faceMs\":%.3f,\"personMs\":%.3f,\"maxRssKiB\":%ld,\"boxes\":[",
            (unsigned long long)h.sequence,(unsigned long long)h.ns,(vision_ns()-h.ns)/1e6,(vision_ns()-start)/1e6,cpu_ms()-c,rotate,resize_ms,face,person,usage.ru_maxrss);
        for(int i=0;i<faces+people;i++){auto v=boxes[i];fprintf(output,"%s{\"kind\":%d,\"score\":%.4f,\"bounds\":[%d,%d,%d,%d]}",i?",":"",v.kind,v.score,v.left,v.top,v.right,v.bottom);}
        fprintf(output,"]}\n");
    }
    close(fd);unlink(vision_path());person_release();face_release();fclose(output);return code;
}
