// Host-only cross-process latest-frame IPC test; no camera, NPU or motors.
#include <array>
#include <cassert>
#include <cstdio>
#include <sys/wait.h>
#include "vision-frame.h"
int main(){
    char dir[]="/tmp/microduck-vision-test-XXXXXX";assert(mkdtemp(dir));
    char path[256];snprintf(path,sizeof(path),"%s/frame",dir);setenv("MICRODUCK_VISION_FRAME",path,1);
    std::array<unsigned char,VISION_BYTES> pixels{};assert(vision_publish(pixels.data(),1));
    int fd=open(path,O_CREAT|O_RDWR,0600);assert(fd>=0);
    assert(!vision_publish(pixels.data(),1));assert(ftruncate(fd,sizeof(VisionHeader)+VISION_BYTES)==0);
    assert(flock(fd,LOCK_EX)==0);assert(vision_publish(pixels.data(),1)); // nonblocking drop
    VisionHeader h{};assert(vision_io(fd,&h,sizeof(h),0,false));assert(h.magic==0);flock(fd,LOCK_UN);
    pid_t pid=fork();assert(pid>=0);
    if(pid==0){close(fd);for(unsigned n=1;n<=100;n++){pixels.fill(n);assert(vision_publish(pixels.data(),n));usleep(1000);} _exit(0);}
    unsigned reads=0;for(int n=0;n<200;n++){assert(flock(fd,LOCK_SH)==0);
        assert(vision_io(fd,&h,sizeof(h),0,false));assert(vision_io(fd,pixels.data(),pixels.size(),sizeof(h),false));flock(fd,LOCK_UN);
        if(h.magic==VisionHeader{}.magic){for(auto v:pixels)assert(v==h.sequence);assert(h.size==VISION_BYTES && h.ns<=vision_ns());reads++;}usleep(500);}
    int status=0;waitpid(pid,&status,0);assert(WIFEXITED(status)&&WEXITSTATUS(status)==0&&reads>0);
    close(fd);unlink(path);rmdir(dir);puts("latest-frame: coherent snapshots, nonblocking contention, missing/invalid file passed");
}
