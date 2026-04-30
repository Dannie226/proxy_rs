#ifndef HTTP_RESULT_H
#define HTTP_RESULT_H

#include "./common.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    size_t _data[2];
} Result;

Result http_res_new_err(const char* str);
Result http_res_new_err_buf(ConstBuffer buf);
Result http_res_new_ok(size_t count);
void http_destroy_res(Result* res);
bool http_res_is_ok(const Result* res);
size_t http_res_get_count(const Result* res);
void http_res_get_err(const Result* res, Buffer* err);

#ifdef __cplusplus
}
#endif
#endif
