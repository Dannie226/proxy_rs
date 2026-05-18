use std::num::NonZeroUsize;

use crate::{
    IsSane, function,
    http2::hpack::tables::{HeaderTable, STATIC_TABLE},
};

#[derive(Default)]
pub struct SipHash<const C: u64, const D: u64> {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    dgst_len: u64,
    dgst_buf: [u8; 8],
}

impl<const C: u64, const D: u64> SipHash<C, D> {
    pub fn init(&mut self, key: [u8; 16]) {
        self.v0 = 0x736f6d6570736575;
        self.v1 = 0x646f72616e646f6d;
        self.v2 = 0x6c7967656e657261;
        self.v3 = 0x7465646279746573;

        let k0 = u64::from_le_bytes(key[0..8].try_into().unwrap());
        let k1 = u64::from_le_bytes(key[8..16].try_into().unwrap());

        self.v3 ^= k1;
        self.v2 ^= k0;
        self.v1 ^= k1;
        self.v0 ^= k0;
        self.dgst_len = 0;
        self.dgst_buf.fill(0);
    }

    #[inline(always)]
    fn sipround(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13);
        self.v1 ^= self.v0;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16);
        self.v3 ^= self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21);
        self.v3 ^= self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17);
        self.v1 ^= self.v2;
        self.v2 = self.v2.rotate_left(32);
    }

    pub fn add(&mut self, mut data: &[u8]) {
        let buf_len = (self.dgst_len % 8) as usize;
        let buf_rem = 8 - buf_len;
        self.dgst_len += data.len() as u64;

        if data.len() < buf_rem {
            self.dgst_buf[buf_len..buf_len + data.len()].copy_from_slice(data);
            return;
        }

        self.dgst_buf[buf_len..].copy_from_slice(&data[..buf_rem]);
        data = &data[buf_rem..];

        let mut m = u64::from_le_bytes(self.dgst_buf);

        self.v3 ^= m;

        for _ in 0..C {
            self.sipround();
        }

        self.v0 ^= m;

        while data.len() >= 8 {
            m = u64::from_le_bytes(data[0..8].try_into().unwrap());

            self.v3 ^= m;

            for _ in 0..C {
                self.sipround();
            }

            self.v0 ^= m;
            data = &data[8..];
        }

        self.dgst_buf.fill(0);
        self.dgst_buf[..data.len()].copy_from_slice(data);
    }

    pub fn digest(&mut self) -> u64 {
        let b = self.dgst_len << 56 | u64::from_le_bytes(self.dgst_buf);

        self.v3 ^= b;

        for _ in 0..C {
            self.sipround();
        }

        self.v0 ^= b;

        self.v2 ^= 0xFF;

        for _ in 0..D {
            self.sipround();
        }

        self.v0 ^ self.v1 ^ self.v2 ^ self.v3
    }
}

const HEADER_MAP_SIZE: u64 = 48;
const HEADER_MAP: [(&[u8], usize); HEADER_MAP_SIZE as usize] = [
    (b"allow", 21),
    (b"content-range", 29),
    (b"authorization", 22),
    (b"cache-control", 23),
    (b"etag", 33),
    (b"date", 32),
    (b"range", 49),
    (b"www-authenticate", 60),
    (b"host", 37),
    (b"from", 36),
    (b"accept-ranges", 17),
    (b"proxy-authenticate", 47),
    (b"content-language", 26),
    (b"if-match", 38),
    (b"if-none-match", 40),
    (b"proxy-authorization", 48),
    (b"cookie", 31),
    (b"accept-language", 16),
    (b"expires", 35),
    (b"via", 59),
    (b"content-length", 27),
    (b"content-disposition", 24),
    (b"if-unmodified-since", 42),
    (b"server", 53),
    (b"vary", 58),
    (b"strict-transport-security", 55),
    (b"accept-charset", 14),
    (b"age", 20),
    (b":status", 7),
    (b"link", 44),
    (b"location", 45),
    (b"user-agent", 57),
    (b"referer", 50),
    (b"if-modified-since", 39),
    (b"refresh", 51),
    (b"content-encoding", 25),
    (b"retry-after", 52),
    (b"content-location", 28),
    (b"transfer-encoding", 56),
    (b"accept-encoding", 15),
    (b"access-control-allow-origin", 19),
    (b"expect", 34),
    (b"if-range", 41),
    (b"last-modified", 43),
    (b"content-type", 30),
    (b"set-cookie", 54),
    (b"accept", 18),
    (b"max-forwards", 46),
];

const MAX_HEADER_LENGTH: usize = 27;
static HEADER_MAP_KEY: [u8; 16] = [
    0x87, 0xf1, 0x5a, 0x17, 0x7a, 0x48, 0x42, 0x45, 0x51, 0x18, 0x8f, 0x69, 0xdc, 0xbf, 0x9a, 0x82,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Setting {
    pub num_flags: u32,
    pub value: u32,
}

impl Setting {
    pub const OPTIONAL_FLAG: u32 = 0x1;
    pub const EXISTS_FLAG: u32 = 0x2;

    pub fn new(num: u16, value: u32) -> Self {
        Setting {
            num_flags: (num as u32) << 16 | Self::EXISTS_FLAG,
            value: value,
        }
    }

    pub fn new_optional(num: u16, value: Option<u32>) -> Self {
        let mut flags = Self::OPTIONAL_FLAG;

        if value.is_some() {
            flags |= Self::EXISTS_FLAG;
        }

        Setting {
            num_flags: (num as u32) << 16 | flags,
            value: value.unwrap_or(0),
        }
    }

    pub fn num(self) -> u16 {
        (self.num_flags >> 16) as u16
    }

    pub fn value(self) -> u32 {
        self.value
    }

    pub fn set_value(&mut self, value: u32) {
        self.num_flags |= Self::EXISTS_FLAG;
        self.value = value;
    }

    pub fn is_optional(self) -> bool {
        (self.num_flags & Self::OPTIONAL_FLAG) != 0
    }

    pub fn exists(self) -> bool {
        (self.num_flags & Self::EXISTS_FLAG) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Settings {
    settings: [Setting; 6],
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            settings: [
                Setting::new(1, 4096),
                Setting::new(2, 1),
                Setting::new_optional(3, None),
                Setting::new(4, (1 << 16) - 1),
                Setting::new(5, 1 << 14),
                Setting::new_optional(6, None),
            ],
        }
    }
}

impl Settings {
    pub fn get(&self, name: SettingName) -> Setting {
        self.settings[name as usize - 1]
    }

    pub fn get_mut(&mut self, name: SettingName) -> &mut Setting {
        &mut self.settings[name as usize - 1]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SettingName {
    HeaderTableSize = 1,
    EnablePush = 2,
    MaxConcurrentStreams = 3,
    InitialWindowSize = 4,
    MaxFrameSize = 5,
    MaxHeaderListSize = 6,
}

impl SettingName {
    pub const ALL: [SettingName; 6] = [
        SettingName::HeaderTableSize,
        SettingName::EnablePush,
        SettingName::MaxConcurrentStreams,
        SettingName::InitialWindowSize,
        SettingName::MaxFrameSize,
        SettingName::MaxHeaderListSize,
    ];

    pub const fn is_valid(num: u16) -> bool {
        num >= Self::HeaderTableSize as u16 && num <= Self::MaxHeaderListSize as u16
    }
}

impl TryFrom<u16> for SettingName {
    type Error = ();

    fn try_from(num: u16) -> Result<Self, Self::Error> {
        match num {
            1 => Ok(Self::HeaderTableSize),
            2 => Ok(Self::EnablePush),
            3 => Ok(Self::MaxConcurrentStreams),
            4 => Ok(Self::InitialWindowSize),
            5 => Ok(Self::MaxFrameSize),
            6 => Ok(Self::MaxHeaderListSize),
            _ => Err(()),
        }
    }
}

pub struct Context {
    pub(crate) hash: SipHash<2, 4>,

    pub(crate) settings: Settings,
    pub(crate) header_table: HeaderTable,
}

impl Context {
    pub(crate) fn new() -> *mut Context {
        Box::into_raw(Box::new(Self::new_no_alloc()))
    }

    pub(crate) fn new_no_alloc() -> Context {
        let settings = Settings::default();
        let header_table =
            HeaderTable::new(settings.get(SettingName::HeaderTableSize).value() as usize);

        Context {
            hash: SipHash::default(),
            header_table: header_table,
            settings: settings,
        }
    }

    fn get_static_header_index(&mut self, name: &[u8], value: &[u8]) -> Option<NonZeroUsize> {
        if name.len() == 0 {
            return None;
        }

        if name.len() > MAX_HEADER_LENGTH {
            return None;
        }

        fn find_index(ctx: &mut Context, header: &[u8]) -> Option<usize> {
            let index = {
                ctx.hash.init(HEADER_MAP_KEY);
                ctx.hash.add(&[header.len() as u8]);
                ctx.hash.add(header);
                let hash = ctx.hash.digest();

                hash % HEADER_MAP_SIZE
            } as usize;

            const MAX_PROBE: usize = 3;
            const MAX_CHECK_LEN: usize = 5;

            'outer: for i in 0..MAX_PROBE {
                let index = (index + i) % (HEADER_MAP_SIZE as usize);

                for i in 0..header
                    .len()
                    .min(HEADER_MAP[index].0.len())
                    .min(MAX_CHECK_LEN)
                {
                    if header[i] != HEADER_MAP[index].0[i] {
                        continue 'outer;
                    }
                }

                return Some(index).filter(|_| HEADER_MAP[index].0 == header);
            }

            None
        }

        let index = find_index(self, name)?;

        let header = HEADER_MAP[index];
        let index = header.1;

        let header = &STATIC_TABLE[index];

        if header.value.is_none() {
            return NonZeroUsize::new(index + 1);
        }

        let mut test_index = index;

        // All the headers with values are near the top of the table,
        // so this index will be small, and incrementing it
        // will still keep it in bounds
        while STATIC_TABLE[test_index].name == name {
            if STATIC_TABLE[test_index].value == Some(value) {
                return NonZeroUsize::new(test_index + 1);
            }

            test_index += 1;
        }

        NonZeroUsize::new(index + 1)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_new_context() -> *mut Context {
    Context::new()
}

/// SAFETY:
///
/// 1) Context must be convertible to a reference
/// 2) Context must have been created from http_new_context
/// 3) Context must not be used after the call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_destroy_context(context: *mut Context) {
    assert!(
        context.is_sane(),
        "{}: Context is not convertible to a reference",
        function!()
    );

    drop(unsafe { Box::from_raw(context) });
}

#[cfg(test)]
mod tests {
    use bstr::BStr;

    use super::*;

    #[test]
    fn test_static_header_index() {
        let mut context = Context::new_no_alloc();

        for header in STATIC_TABLE.iter().take(7) {
            let index = context.get_static_header_index(header.name, header.value.unwrap_or(&[]));

            assert!(index.is_none(), "header: {}", BStr::new(header.name));
        }

        for (i, header) in STATIC_TABLE.iter().enumerate().skip(7) {
            let index = context.get_static_header_index(header.name, header.value.unwrap_or(&[]));

            assert_eq!(
                index,
                NonZeroUsize::new(i + 1),
                "header: {}",
                BStr::new(header.name)
            );
        }

        assert_eq!(
            context.get_static_header_index(b":status", b""),
            NonZeroUsize::new(8)
        );

        assert_eq!(
            context.get_static_header_index(b"accept-encoding", b""),
            NonZeroUsize::new(16)
        );
    }

    #[test]
    fn test_non_header_index() {
        let mut context = Context::new_no_alloc();

        for header in [
            "look-at-all-those-chickens",
            "content-encodin",
            "accept-lang",
            "x-real-ip",
        ] {
            let index = context.get_static_header_index(header.as_bytes(), &[]);

            assert!(index.is_none());
        }
    }
}
