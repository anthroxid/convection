use std::{collections::HashMap, fs, path::PathBuf, sync::Mutex};

use image::ImageFormat;
use log::{debug, trace, warn};

use crate::{
    factory::{DummyTileFactory, TileFactory},
    tile::{Tile, TileImage, TilingScheme},
};

pub trait TileCache {
    fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>>;
    fn put(&self, key: &str, bytes: &[u8]) -> anyhow::Result<()>;
}

#[derive(Default)]
pub struct MemoryCache {
    entries: Mutex<HashMap<String, Vec<u8>>>,
}

impl TileCache for MemoryCache {
    fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn put(&self, key: &str, bytes: &[u8]) -> anyhow::Result<()> {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
        Ok(())
    }
}

pub struct FsCache {
    root: PathBuf,
}

impl FsCache {
    /// create new fs cache using a root path as starting point,
    /// ensure the directory exists before using
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        debug!("tile cache rooted at {}", root.display());
        Self { root }
    }

    /// cache under the system temp dir, e.g. /tmp/tile-cache on Linux
    pub fn tmp() -> Self {
        Self::new(std::env::temp_dir().join("tile-cache"))
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.png"))
    }
}

impl Default for FsCache {
    fn default() -> Self {
        Self::tmp()
    }
}

impl TileCache for FsCache {
    fn get(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match fs::read(self.path_for(key)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        Ok(())
    }
}

/// a caching tile factory that wraps both an inner `TileFactory`
/// as well as the paired `TileCache` implementation, providing
/// "free" caching on top of the existing trait implementations
pub struct CachingTileFactory<F: TileFactory, C: TileCache> {
    inner: F,
    cache: C,
}

impl<F: TileFactory, C: TileCache> CachingTileFactory<F, C> {
    /// wrap both the `TileFactory` and `TileCache` with the caching
    /// tile factory
    pub fn wrap(inner: F, cache: C) -> Self {
        Self { inner, cache }
    }

    fn key_for(&self, tile: Tile) -> String {
        let scheme_id = self.inner.scheme().id();
        let zoom = tile.zoom();
        let (x, y) = tile.xy();
        if let Some(ns) = self.inner.cache_namespace() {
            format!("{scheme_id}/{ns}/{zoom}/{x}/{y}")
        } else {
            format!("{scheme_id}/{zoom}/{x}/{y}")
        }
    }
}

impl<F, C> TileFactory for CachingTileFactory<F, C>
where
    F: TileFactory,
    C: TileCache + Default,
{
    type Scheme = F::Scheme;

    fn new() -> Self {
        Self {
            inner: F::new(),
            cache: C::default(),
        }
    }

    fn with_scheme(scheme: Self::Scheme) -> Self {
        Self {
            inner: F::with_scheme(scheme),
            cache: C::default(),
        }
    }

    fn scheme(&self) -> &Self::Scheme {
        self.inner.scheme()
    }

    fn zoom_range(&self) -> std::ops::RangeInclusive<u32> {
        self.inner.zoom_range()
    }

    fn cache_namespace(&self) -> Option<String> {
        self.inner.cache_namespace()
    }

    fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
        let key = self.key_for(tile);

        match self.cache.get(&key) {
            Ok(Some(bytes)) => match image::load_from_memory(&bytes) {
                Ok(image) => {
                    trace!("cache hit for {key} ({} bytes)", bytes.len());
                    return Ok(TileImage::new(tile, image.into_rgba8()));
                }
                // a corrupt entry is not worth failing the tile over, the
                // inner factory can produce it again
                Err(err) => warn!("discarding unreadable cache entry {key}: {err}"),
            },
            Ok(None) => trace!("cache miss for {key}"),
            // likewise, a cache that cannot be read is not a reason to give
            // up on the tile itself
            Err(err) => warn!("failed to read cache entry {key}: {err:#}"),
        }

        let tile_image = self.inner.rendered_tile(tile)?;

        let mut buf = std::io::Cursor::new(Vec::new());
        tile_image.image().write_to(&mut buf, ImageFormat::Png)?;
        if let Err(err) = self.cache.put(&key, buf.get_ref()) {
            warn!("failed to cache {key}: {err:#}");
        } else {
            trace!("cached {key} ({} bytes)", buf.get_ref().len());
        }

        Ok(tile_image)
    }
}

pub type MemCachedDummyTileFactory = CachingTileFactory<DummyTileFactory, MemoryCache>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_dummy_tile_factory() -> anyhow::Result<()> {
        let factory = MemCachedDummyTileFactory::new();
        let tile1 = factory.tile_at(47.26767475658791, 11.423860146362568, 4);
        // none rendered yet
        assert_eq!(factory.cache.entries.lock().unwrap().len(), 0);

        let _ = factory.rendered_tile(tile1)?;
        assert_eq!(factory.cache.entries.lock().unwrap().len(), 1);

        // should stay size one
        let tile2 = factory.tile_at(47.26767475658791, 11.423860146362568, 4);
        let tile_image = factory.rendered_tile(tile2)?;
        assert_eq!(factory.cache.entries.lock().unwrap().len(), 1);

        let tile3 = factory.tile_at(0., 0., 1);
        let tile_image1 = factory.rendered_tile(tile3)?;
        assert_eq!(factory.cache.entries.lock().unwrap().len(), 2);

        assert_ne!(tile_image, tile_image1);
        Ok(())
    }
}
