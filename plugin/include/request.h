#ifndef HTTP_REQUEST_H
#define HTTP_REQUEST_H

#include "./common.h"
#include "./result.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct __Request Request;

void http_read_body(Request* request, uchar* data, size_t len, Result* res);
void http_get_method(const Request* request, Buffer* buf);
void http_get_uri(const Request* request, Buffer* buf);
void http_get_version(const Request* request, uint32_t* major, uint32_t* minor);
size_t http_get_header_count(const Request* request, ConstBuffer name);
int http_get_header(const Request* request, ConstBuffer name, size_t index, Buffer* value);
void http_destroy_request(Request* request);

#ifdef __cplusplus
}
#endif
#endif
