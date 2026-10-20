#pragma once
#include "common.h"
struct VisionBox {int kind; float score; int left,top,right,bottom;};
// Separate translation units preserve the two upstream model context ABIs.
int person_init(const char* path);
int face_init(const char* path);
int person_run(image_buffer_t* image,VisionBox* boxes,int capacity);
int face_run(image_buffer_t* image,VisionBox* boxes,int capacity);
void person_release();
void face_release();
