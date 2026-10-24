// Single latest corrected RGB frame. flock prevents torn reads; writers never wait.
#pragma once
#include <cstdint>
#include <cerrno>
#include <cstdlib>
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
constexpr size_t VISION_BYTES=640*480*3;
struct VisionHeader { uint64_t magic=0x4d44564953303032ULL, sequence=0, ns=0, size=VISION_BYTES; };
inline uint64_t vision_ns() {timespec t{};clock_gettime(CLOCK_MONOTONIC,&t);return uint64_t(t.tv_sec)*1000000000ULL+t.tv_nsec;}
inline const char* vision_path() {auto p=getenv("MICRODUCK_VISION_FRAME");return p?p:"/run/microduck-observer/vision.rgb";}
inline bool vision_io(int fd,void* p,size_t size,off_t at,bool write_data) {
    auto bytes=static_cast<unsigned char*>(p);
    while(size){ssize_t n=write_data?pwrite(fd,bytes,size,at):pread(fd,bytes,size,at);
        if(n<0 && errno==EINTR)continue;
        if(n<=0)return false;
        bytes+=n;size-=n;at+=n;}
    return true;
}
inline bool vision_publish(void* rgb,uint64_t sequence) {
    int fd=open(vision_path(),O_WRONLY|O_CLOEXEC|O_NOFOLLOW);
    if(fd<0)return errno==ENOENT; // No consumer: no copy, allocation or inference.
    bool ok=true;struct stat st{};
    if(fstat(fd,&st)!=0 || !S_ISREG(st.st_mode) || st.st_size!=off_t(sizeof(VisionHeader)+VISION_BYTES))ok=false;
    else if(flock(fd,LOCK_EX|LOCK_NB)==0){VisionHeader h;h.sequence=sequence;h.ns=vision_ns();
        ok=vision_io(fd,rgb,VISION_BYTES,sizeof(h),true)&&vision_io(fd,&h,sizeof(h),0,true);flock(fd,LOCK_UN);
    }else if(errno!=EWOULDBLOCK)ok=false;
    close(fd);return ok;
}
