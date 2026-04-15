#[macro_export]
macro_rules! opaque_type {
    ($($name: ident),+) => {
        $(#[repr(C)]
        pub struct $name {
            _data: (),
            _marker: ::core::marker::PhantomData<(*mut u8, ::core::marker::PhantomPinned)>,
        }
        )+
    };
}
