#ifndef HTTP_COMMON_H
#define HTTP_COMMON_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif
typedef unsigned char uchar;

typedef struct {
    size_t len;
    uchar* buf;
} Buffer;

typedef struct {
    size_t len;
    const uchar* buf;
} ConstBuffer;

#ifdef __cplusplus
}
#endif
#endif
