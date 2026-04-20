use std::{
    borrow::Cow,
    collections::{VecDeque, vec_deque::Iter},
    iter::repeat_n,
};

use bstr::{BStr, BString};

pub struct StaticEntry {
    pub name: &'static [u8],
    pub value: Option<&'static [u8]>,
}

impl StaticEntry {
    const fn new(name: &'static [u8], value: Option<&'static [u8]>) -> StaticEntry {
        StaticEntry { name, value }
    }
}

pub static STATIC_TABLE: [StaticEntry; 61] = [
    StaticEntry::new(b":authority", None),
    StaticEntry::new(b":method", Some(b"GET")),
    StaticEntry::new(b":method", Some(b"POST")),
    StaticEntry::new(b":path", Some(b"/")),
    StaticEntry::new(b":path", Some(b"/index.html")),
    StaticEntry::new(b":scheme", Some(b"http")),
    StaticEntry::new(b":scheme", Some(b"https")),
    StaticEntry::new(b":status", Some(b"200")),
    StaticEntry::new(b":status", Some(b"204")),
    StaticEntry::new(b":status", Some(b"206")),
    StaticEntry::new(b":status", Some(b"304")),
    StaticEntry::new(b":status", Some(b"400")),
    StaticEntry::new(b":status", Some(b"404")),
    StaticEntry::new(b":status", Some(b"500")),
    StaticEntry::new(b"accept-charset", None),
    StaticEntry::new(b"accept-encoding", Some(b"gzip, deflate")),
    StaticEntry::new(b"accept-language", None),
    StaticEntry::new(b"accept-ranges", None),
    StaticEntry::new(b"accept", None),
    StaticEntry::new(b"access-control-allow-origin", None),
    StaticEntry::new(b"age", None),
    StaticEntry::new(b"allow", None),
    StaticEntry::new(b"authorization", None),
    StaticEntry::new(b"cache-control", None),
    StaticEntry::new(b"content-disposition", None),
    StaticEntry::new(b"content-encoding", None),
    StaticEntry::new(b"content-language", None),
    StaticEntry::new(b"content-length", None),
    StaticEntry::new(b"content-location", None),
    StaticEntry::new(b"content-range", None),
    StaticEntry::new(b"content-type", None),
    StaticEntry::new(b"cookie", None),
    StaticEntry::new(b"date", None),
    StaticEntry::new(b"etag", None),
    StaticEntry::new(b"expect", None),
    StaticEntry::new(b"expires", None),
    StaticEntry::new(b"from", None),
    StaticEntry::new(b"host", None),
    StaticEntry::new(b"if-match", None),
    StaticEntry::new(b"if-modified-since", None),
    StaticEntry::new(b"if-none-match", None),
    StaticEntry::new(b"if-range", None),
    StaticEntry::new(b"if-unmodified-since", None),
    StaticEntry::new(b"last-modified", None),
    StaticEntry::new(b"link", None),
    StaticEntry::new(b"location", None),
    StaticEntry::new(b"max-forwards", None),
    StaticEntry::new(b"proxy-authenticate", None),
    StaticEntry::new(b"proxy-authorization", None),
    StaticEntry::new(b"range", None),
    StaticEntry::new(b"referer", None),
    StaticEntry::new(b"refresh", None),
    StaticEntry::new(b"retry-after", None),
    StaticEntry::new(b"server", None),
    StaticEntry::new(b"set-cookie", None),
    StaticEntry::new(b"strict-transport-security", None),
    StaticEntry::new(b"transfer-encoding", None),
    StaticEntry::new(b"user-agent", None),
    StaticEntry::new(b"vary", None),
    StaticEntry::new(b"via", None),
    StaticEntry::new(b"www-authenticate", None),
];

pub struct DynamicTable {
    buffer: VecDeque<u8>,
    max_size: usize,
    num_elements: usize,
}

impl DynamicTable {
    pub fn new(max_permitted_size: usize) -> DynamicTable {
        DynamicTable {
            buffer: VecDeque::with_capacity(max_permitted_size),
            max_size: max_permitted_size,
            num_elements: 0,
        }
    }

    fn get_size(&self, offset: usize) -> u64 {
        let mut size = [0u8; 8];

        self.buffer
            .range(offset..offset + 8)
            .copied()
            .enumerate()
            .for_each(|(i, v)| size[i] = v);

        u64::from_le_bytes(size)
    }

    pub fn insert(&mut self, name: &[u8], value: &[u8]) {
        let size = name.len() + value.len() + 32;

        while self.buffer.len() + size > self.max_size {
            if self.buffer.len() == 0 {
                return;
            }

            let size = self.get_size(0) as usize;

            self.buffer.drain(0..size);
            self.num_elements -= 1;
        }

        self.buffer.extend((size as u64).to_le_bytes());
        self.buffer.extend((name.len() as u64).to_le_bytes());
        self.buffer.extend((value.len() as u64).to_le_bytes());
        self.buffer.extend(name);
        self.buffer.extend(value);
        // Size on both sides for double ended iteration
        self.buffer.extend((size as u64).to_le_bytes());
        self.num_elements += 1;
    }

    pub fn get(&self, index: usize) -> Option<(Iter<'_, u8>, Iter<'_, u8>)> {
        if index >= self.num_elements {
            return None;
        }

        let mut offset = self.buffer.len();

        for _ in 0..index {
            offset -= self.get_size(offset - 8) as usize;
        }

        offset -= self.get_size(offset - 8) as usize;

        let name_size = self.get_size(offset + 8) as usize;
        let value_size = self.get_size(offset + 16) as usize;

        offset += 24;

        let name_iter = self.buffer.range(offset..offset + name_size);
        offset += name_size;
        let value_iter = self.buffer.range(offset..offset + value_size);

        Some((name_iter, value_iter))
    }
}

pub struct HeaderTable(DynamicTable);

impl HeaderTable {
    pub fn new(max_table_size: usize) -> HeaderTable {
        HeaderTable(DynamicTable::new(max_table_size))
    }

    pub fn get(
        &self,
        index: usize,
        get_value: bool,
    ) -> Option<(Cow<'static, BStr>, Option<BString>)> {
        if index == 0 {
            None
        } else if index <= 61 {
            Some((
                Cow::Borrowed(STATIC_TABLE[index - 1].name.into()),
                STATIC_TABLE[index - 1]
                    .value
                    .filter(|_| get_value)
                    .map(Into::into),
            ))
        } else {
            self.0.get(index - 62).map(|(n, v)| {
                (
                    Cow::Owned(n.copied().collect()),
                    Some(v)
                        .filter(|_| get_value)
                        .map(Iterator::copied)
                        .map(Iterator::collect),
                )
            })
        }
    }

    pub fn insert(&mut self, name: &[u8], value: &[u8]) {
        self.0.insert(name, value)
    }

    pub fn dyn_size(&self) -> usize {
        self.0.num_elements
    }
}
