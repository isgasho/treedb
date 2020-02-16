use crate::{page::Page, Error, Result};
use lru::LruCache;
use std::fmt;
use std::fs::File;
use std::os::unix::fs::FileExt;

#[derive(Debug)]
pub struct Pager {
    file: File,
    lru: LruCache<PageId, CachedPage>,
    next_page: usize,
    cache_size: usize,
    page_size: usize,
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct PageId(usize);

#[derive(Debug)]
pub struct CachedPage {
    id: PageId,
    inner: Page,
    dirty: bool,
}

impl Pager {
    pub fn new(page_size: usize, file: File, cache_size: usize) -> Self {
        let lru = LruCache::new(cache_size);

        Pager {
            file,
            cache_size,
            page_size,
            next_page: 0,
            lru,
        }
    }

    pub fn alloc_page(&mut self, page: Page) -> Result<&mut CachedPage> {
        // TODO: add usize::max check

        let page_id = PageId(self.next_page);

        self.next_page += 1;

        let page = CachedPage {
            id: page_id,
            inner: page,
            dirty: true,
        };

        self.pop_lru()?;

        self.lru.put(page_id, page);

        if let Some(page) = self.lru.get_mut(&page_id) {
            Ok(page)
        } else {
            unreachable!("page is statically set")
        }
    }

    pub fn get(&mut self, page_id: impl Into<PageId>) -> Result<&mut CachedPage> {
        let page_id = page_id.into();

        if page_id.0 >= self.next_page {
            return Err(Error::IndexOutofBounds(page_id));
        }

        // Check if we need to page in
        if let None = self.lru.peek(&page_id) {
            let offset = self.page_size * usize::from(page_id);

            self.pop_lru()?;

            let mut buf = vec![0; self.page_size];

            self.file.read_exact_at(&mut buf[..], offset as u64)?;

            let inner = Page::deserialize(self.page_size, buf)?;

            let page = CachedPage {
                id: page_id,
                inner,
                dirty: false,
            };

            self.lru.put(page_id, page);
        }

        if let Some(page) = self.lru.get_mut(&page_id) {
            Ok(page)
        } else {
            unreachable!("page is statically set")
        }
    }

    fn pop_lru(&mut self) -> Result<()> {
        if self.lru.len() == self.lru.cap() {
            if let Some((_page_id, page)) = self.lru.pop_lru() {
                Pager::flush(self.page_size, &self.file, &page)?;
            }
        }

        Ok(())
    }

    fn flush(page_size: usize, file: &File, page: &CachedPage) -> Result<()> {
        let offset = page_size * usize::from(page.id);

        let data = page.page().serialize()?;
        file.write_all_at(&data[..], offset as u64)?;

        Ok(())
    }

    pub fn evict(&mut self, page_id: impl Into<PageId>) -> Result<()> {
        let page_id = page_id.into();

        if let Some(page) = self.lru.pop(&page_id) {
            Pager::flush(self.page_size, &self.file, &page)?;
        }

        Ok(())
    }

    pub fn flush_all(&mut self) -> Result<()> {
        for (_, page) in &self.lru {
            if page.dirty {
                Pager::flush(self.page_size, &self.file, &page)?;
            }
        }

        Ok(())
    }
}

impl CachedPage {
    pub fn id(&self) -> PageId {
        self.id
    }

    pub fn page_mut(&mut self) -> &mut Page {
        self.dirty = true;
        &mut self.inner
    }

    pub fn page(&self) -> &Page {
        &self.inner
    }
}

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<PageId> for usize {
    fn from(t: PageId) -> Self {
        t.0 as usize
    }
}

impl From<usize> for PageId {
    fn from(t: usize) -> Self {
        PageId(t)
    }
}

impl From<&PageId> for PageId {
    fn from(t: &PageId) -> Self {
        *t
    }
}
