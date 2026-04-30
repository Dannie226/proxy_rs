#ifndef HTTP_BIO_H
#define HTTP_BIO_H

#include "./result.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct __Reader Reader;
typedef struct __Writer Writer;

typedef void (*ClearFn)(void*);
typedef void (*ReadFn)(void*, void*, size_t, Result*);
typedef void (*WriteFn)(void*, const void*, size_t, Result*);

void http_null_clear(void*);
void http_null_read(void*, void*, size_t, Result*);
void http_null_write(void*, const void*, size_t, Result*);

Reader* http_new_reader(void* data, ReadFn read, ClearFn clear);
Reader* http_new_empty_data_reader(ReadFn read);
void http_destroy_reader(Reader* reader);

Writer* http_new_writer(void* data, WriteFn write, ClearFn clear);
Writer* http_new_empty_data_writer(WriteFn write);
void http_destroy_writer(Writer* writer);

void http_bio_read(Reader* reader, void* data, size_t len, Result* res);
void http_bio_write(Writer* writer, const void* data, size_t len, Result* res);
#ifdef __cplusplus
}
#endif
#endif
