#include <stdlib.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#include "./plugin/include/request.h"
#include "./plugin/include/response.h"

void* init() {
    return NULL;
}

void close(void* _) {

}

static ConstBuffer buffer_from_str(const char* str) {
    size_t len = strlen(str);

    return (ConstBuffer){
        .len = len,
        .buf = (const unsigned char*)str
    };
}

void handle_request(void* _, Request* req, ResponseWriter* writer) {
    Result r = { 0 };

    Buffer uri = { 0 };

    http_get_uri(req, &uri);

    uri.buf = calloc(uri.len + 1, sizeof(uchar));

    http_get_uri(req, &uri);

    printf("URI: %s\n", uri.buf);

    http_get_method(req, &uri);
    uri.buf[uri.len] = 0;

    printf("Method: %s\n", uri.buf);

    free(uri.buf);
    uri = (Buffer){ 0 };

    const char* response = "Hello There. AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!\n"
        "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB!\n"
        "Hiiiiiiiiiiiiiiiiiiiiiiiiiiii!";
    size_t len = strlen(response);

    char buf[20] = { 0 };
    sprintf(buf, "%lu", len);
    // http_add_header(writer, buffer_from_str("content-length"), buffer_from_str(buf));

    http_write(writer, (const uchar*)response, len, &r);

    http_destroy_res(&r);
}
