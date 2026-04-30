use std::{ffi::*, io, mem};

use http_rs::{
    request::{Request, ffi as req_ffi},
    response::{ResponseWriter, ffi as res_ffi},
};

type RequestFn = unsafe extern "C" fn(
    arg: *mut c_void,
    req: *mut req_ffi::Request,
    writer: *mut res_ffi::ResponseWriter,
);

type CloseFn = unsafe extern "C" fn(arg: *mut c_void);

pub struct Plugin {
    data: *mut c_void,
    handle: *mut c_void,
    handle_request: RequestFn,
    close: CloseFn,
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void);
    fn dlerror() -> *mut c_char;
}

fn get_dlerror<T>(message: &str) -> io::Result<T> {
    let msg = unsafe { dlerror() };

    let err = if msg.is_null() {
        message.to_string()
    } else {
        format!(
            "{message}: {}",
            unsafe { CStr::from_ptr(msg) }.to_string_lossy()
        )
    };

    Err(io::Error::other(err))
}

impl Plugin {
    pub unsafe fn load_plugin(path: &CStr) -> io::Result<Plugin> {
        let handle = unsafe { dlopen(path.as_ptr(), 1) };

        if handle.is_null() {
            return get_dlerror("Failed to load plugin");
        }

        let req_fn = unsafe { dlsym(handle, c"handle_request".as_ptr()) };

        if req_fn.is_null() {
            return get_dlerror("Failed to load request handler");
        }

        let req_fn = unsafe { mem::transmute(req_fn) };

        let close_fn = unsafe { dlsym(handle, c"close".as_ptr()) };

        if close_fn.is_null() {
            return get_dlerror("Failed to load close function");
        }

        let close_fn = unsafe { mem::transmute(close_fn) };

        let init_fn = unsafe { dlsym(handle, c"init".as_ptr()) };

        if init_fn.is_null() {
            return get_dlerror("Failed to load initialize function");
        }

        let init_fn =
            unsafe { mem::transmute::<_, unsafe extern "C" fn() -> *mut c_void>(init_fn) };

        let data = unsafe { init_fn() };

        Ok(Plugin {
            data: data,
            handle,
            handle_request: req_fn,
            close: close_fn,
        })
    }

    pub fn handle_request<'a>(&mut self, req: Request<'a>, res: ResponseWriter<'a>) {
        unsafe { (self.handle_request)(self.data, req.get_ffi(), res.get_ffi()) }
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.data);
            dlclose(self.handle);
        };
    }
}
