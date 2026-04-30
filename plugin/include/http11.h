#ifndef HTTP_HTTP11_H
#define HTTP_HTTP11_H

#include "./common.h"
#include "./request.h"
#include "./response.h"

#ifdef __cplusplus
extern "C" {
#endif

Request* http_parse_http11_request(Reader* reader, Buffer* err);
ResponseWriter* http_http11_response_writer(Writer* writer);

#ifdef __cplusplus
}
#endif
#endif
