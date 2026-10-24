// Adapter compiled separately for each pinned Rockchip Zoo v2.3.2 model.
#include "vision-model.h"
#include <algorithm>
#include <cmath>
static bool add_box(VisionBox* boxes,int& n,int kind,float score,int l,int t,int r,int b,const image_buffer_t* img){
    l=std::clamp(l,0,img->width);r=std::clamp(r,0,img->width);t=std::clamp(t,0,img->height);b=std::clamp(b,0,img->height);
    if(!std::isfinite(score)||score>1.f||l>=r||t>=b)return false;
    boxes[n++]={kind,score,l,t,r,b};return true;
}
#ifdef VISION_PERSON
#include "yolov5.h"
static rknn_app_context_t ctx{};
int person_init(const char* path){int rc=init_yolov5_model(path,&ctx);if(rc)return rc;return ctx.model_width==640&&ctx.model_height==640&&ctx.model_channel==3&&ctx.io_num.n_input==1&&ctx.io_num.n_output==3&&ctx.is_quant?0:-1;}
void person_release(){release_yolov5_model(&ctx);}
int person_run(image_buffer_t* img,VisionBox* boxes,int capacity){
    object_detect_result_list result{};
    if(inference_yolov5_model(&ctx,img,&result)!=0)return -1;
    int n=0;for(int i=0;i<result.count && n<capacity;i++){auto& v=result.results[i];
        if(v.cls_id==0 && v.prop>=0.45f)add_box(boxes,n,0,v.prop,v.box.left,v.box.top,v.box.right,v.box.bottom,img);}
    return n;
}
#else
#include "retinaface.h"
static rknn_app_context_t ctx{};
int face_init(const char* path){int rc=init_retinaface_model(path,&ctx);if(rc)return rc;return ctx.model_width==320&&ctx.model_height==320&&ctx.model_channel==3&&ctx.io_num.n_input==1&&ctx.io_num.n_output==3?0:-1;}
void face_release(){release_retinaface_model(&ctx);}
int face_run(image_buffer_t* img,VisionBox* boxes,int capacity){
    retinaface_result result{};
    if(inference_retinaface_model(&ctx,img,&result)!=0)return -1;
    int n=0;for(int i=0;i<result.count && n<capacity;i++){auto& v=result.object[i];
        if(v.score>=0.60f)add_box(boxes,n,1,v.score,v.box.left,v.box.top,v.box.right,v.box.bottom,img);}
    return n;
}
#endif
