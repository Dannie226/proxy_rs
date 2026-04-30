#ifndef HTTP_RESPONSE_H
#define HTTP_RESPONSE_H

#include "./result.h"
#include "./bio.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct __ResponseWriter ResponseWriter;

typedef void (*ResponseWriteFn)(Writer*, const void*, size_t, Result*);
typedef void (*StatusFn)(ResponseWriter*, uint16_t, Result*);

ResponseWriter* http_new_response_writer(Writer* writer, StatusFn status_fn);
ResponseWriter* http_custom_response_writer(Writer* writer, StatusFn status_fn, ResponseWriteFn write_fn);

void http_write(ResponseWriter* writer, const uchar* data, size_t len, Result* res);
void http_add_header(ResponseWriter* writer, ConstBuffer name, ConstBuffer value);
void http_remove_header(ResponseWriter* writer, ConstBuffer name);
void http_write_status(ResponseWriter* writer, uint16_t status, Result* res);
void http_destroy_response_writer(ResponseWriter* writer);

#ifdef __cplusplus
}
#endif
#endif
