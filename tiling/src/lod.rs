use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, Sender},
    },
};

use convection_types::{Camera, Globe};
use typed_builder::TypedBuilder;

use crate::{
    factory::TileFactory,
    tile::{Tile, TileImage, TilingScheme},
};

#[derive(TypedBuilder, Clone, Copy)]
pub struct LodConfig {
    pub target_pixel_error: f64,
    pub min_zoom: u32,
    pub max_zoom: u32,
    pub max_concurrent_loads: usize,
    pub cache_capacity: usize,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            target_pixel_error: 2.0,
            min_zoom: 0,
            max_zoom: 18,
            max_concurrent_loads: 8,
            cache_capacity: 512,
        }
    }
}

// quadtree traversal

fn tile_center_lonlat(scheme: &impl TilingScheme, tile: Tile) -> (f64, f64) {
    let [west, east, south, north] = scheme.bounds(tile).inner();
    ((west + east) / 2.0, (south + north) / 2.0)
}

/// projected size (px) of this tile's ground footprint
/// as seen by the camera
fn screen_space_error(
    scheme: &impl TilingScheme,
    tile: Tile,
    globe: &Globe,
    camera: &Camera,
) -> f64 {
    let (lon, lat) = tile_center_lonlat(scheme, tile);
    let p = globe.lonlat_to_point(lon, lat);
    let dist = (p - camera.position).length().max(1.0);
    let geometric_error_m = scheme.approx_tile_size(tile.zoom()).as_meters();

    (geometric_error_m * camera.viewport_height_px as f64)
        / (dist * 2.0 * (camera.fov_y_rad / 2.0).tan())
}

/// backface culling to ignore the backside of the globe, should of course
/// also be implemeneted in the renderer (see frustum culling)
fn is_facing_camera(
    scheme: &impl TilingScheme,
    tile: Tile,
    globe: &Globe,
    camera: &Camera,
) -> bool {
    let (lon, lat) = tile_center_lonlat(scheme, tile);
    let p = globe.lonlat_to_point(lon, lat);
    let normal = globe.normal_at(lon, lat);
    let to_camera = (camera.position - p).normalize();
    normal.dot(to_camera) > -0.05 // small slack so grazing horizon tiles still load
}

/// recursively select the tile set to render this frame
fn select_tiles(
    scheme: &impl TilingScheme,
    globe: &Globe,
    camera: &Camera,
    config: &LodConfig,
    effective_max_zoom: u32,
    out: &mut Vec<Tile>,
) {
    fn recurse(
        scheme: &impl TilingScheme,
        globe: &Globe,
        camera: &Camera,
        config: &LodConfig,
        effective_max_zoom: u32,
        tile: Tile,
        out: &mut Vec<Tile>,
    ) {
        if !is_facing_camera(scheme, tile, globe, camera) {
            return;
        }

        let sse = screen_space_error(scheme, tile, globe, camera);
        let can_refine = tile.zoom() < effective_max_zoom;
        let should_refine = can_refine && sse > config.target_pixel_error;

        if should_refine {
            for child in tile.children() {
                recurse(
                    scheme,
                    globe,
                    camera,
                    config,
                    effective_max_zoom,
                    child,
                    out,
                );
            }
        } else {
            out.push(tile);
        }
    }

    for root in scheme.root_tiles() {
        if root.zoom() >= config.min_zoom {
            recurse(scheme, globe, camera, config, effective_max_zoom, root, out);
        } else {
            for child in root.children() {
                recurse(
                    scheme,
                    globe,
                    camera,
                    config,
                    effective_max_zoom,
                    child,
                    out,
                );
            }
        }
    }
}

#[derive(Clone)]
enum CacheEntry {
    Loading,
    Ready(Arc<TileImage>),
    Failed,
}

struct CacheSlot {
    entry: CacheEntry,
    last_used_frame: u64,
}

/// LOD tile source to be used by the renderer
pub enum LodTileSource {
    Exact(Arc<TileImage>),
    /// tile isn't loaded yet, so sample the ancestor (cropping by uv_rect)
    /// instead
    Fallback {
        image: Arc<TileImage>,
        uv_rect: (f32, f32, f32, f32),
    },
    None,
}

pub struct LodTile {
    pub tile: Tile,
    pub source: LodTileSource,
}

pub struct LodTileManager<F: TileFactory + Send + Sync + 'static> {
    factory: Arc<F>,
    #[allow(unused)]
    scheme_id: &'static str,
    config: LodConfig,
    effective_max_zoom: u32,

    cache: Mutex<HashMap<Tile, CacheSlot>>,
    inflight: AtomicUsize,
    frame_counter: AtomicU64,

    result_tx: Sender<(Tile, CacheEntry)>,
    result_rx: Mutex<Receiver<(Tile, CacheEntry)>>,
}

impl<F: TileFactory + Send + Sync + 'static> LodTileManager<F> {
    pub fn new(factory: F, config: LodConfig) -> Self {
        let scheme_id = factory.scheme().id();
        let (result_tx, result_rx) = std::sync::mpsc::channel();

        Self {
            factory: Arc::new(factory),
            scheme_id,
            effective_max_zoom: config.max_zoom,
            config,
            cache: Mutex::new(HashMap::new()),
            inflight: AtomicUsize::new(0),
            frame_counter: AtomicU64::new(0),
            result_tx,
            result_rx: Mutex::new(result_rx),
        }
    }

    /// update per-frame
    pub fn update(&self, camera: &Camera, globe: &Globe) -> Vec<LodTile>
    where
        F::Scheme: 'static,
    {
        let frame = self.frame_counter.fetch_add(1, Ordering::Relaxed);

        self.drain_completed_loads();

        let scheme = self.factory.scheme();
        let mut desired = Vec::new();
        select_tiles(
            scheme,
            globe,
            camera,
            &self.config,
            self.effective_max_zoom,
            &mut desired,
        );

        let mut visible = Vec::with_capacity(desired.len());
        for tile in desired {
            let source = self.resolve_source(tile, frame);
            self.maybe_request_load(tile);
            visible.push(LodTile { tile, source });
        }

        self.evict_if_needed(frame);
        visible
    }

    fn resolve_source(&self, tile: Tile, frame: u64) -> LodTileSource {
        let mut cache = self.cache.lock().unwrap();

        if let Some(slot) = cache.get_mut(&tile) {
            slot.last_used_frame = frame;
            if let CacheEntry::Ready(img) = &slot.entry {
                return LodTileSource::Exact(img.clone());
            }
        }

        // walk ancestors for a "ready" fallback image
        let mut cur = tile;
        let mut depth = 0u32;
        while let Some(parent) = cur.parent() {
            depth += 1;
            if let Some(slot) = cache.get_mut(&parent) {
                slot.last_used_frame = frame;
                if let CacheEntry::Ready(img) = &slot.entry {
                    let scale = 1u32 << depth;
                    let lx = tile.x() - parent.x() * scale;
                    let ly = tile.y() - parent.y() * scale;
                    let uv_rect = (
                        lx as f32 / scale as f32,
                        ly as f32 / scale as f32,
                        (lx + 1) as f32 / scale as f32,
                        (ly + 1) as f32 / scale as f32,
                    );
                    return LodTileSource::Fallback {
                        image: img.clone(),
                        uv_rect,
                    };
                }
            }
            cur = parent;
        }

        LodTileSource::None
    }

    fn maybe_request_load(&self, tile: Tile) {
        {
            let cache = self.cache.lock().unwrap();
            if cache.contains_key(&tile) {
                // already loading, ready, or failed
                return;
            }
        }
        if self.inflight.load(Ordering::Relaxed) >= self.config.max_concurrent_loads {
            // backpressure, try again next frame
            return;
        }

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(
                tile,
                CacheSlot {
                    entry: CacheEntry::Loading,
                    last_used_frame: 0,
                },
            );
        }
        self.inflight.fetch_add(1, Ordering::Relaxed);

        let factory = self.factory.clone();
        let tx = self.result_tx.clone();
        // TODO: use thread pool/executor here
        std::thread::spawn(move || {
            let entry = match factory.rendered_tile(tile) {
                Ok(img) => CacheEntry::Ready(Arc::new(img)),
                Err(_) => CacheEntry::Failed,
            };
            let _ = tx.send((tile, entry));
        });
    }

    fn drain_completed_loads(&self) {
        let rx = self.result_rx.lock().unwrap();
        let mut cache = self.cache.lock().unwrap();
        while let Ok((tile, entry)) = rx.try_recv() {
            self.inflight.fetch_sub(1, Ordering::Relaxed);
            cache.insert(
                tile,
                CacheSlot {
                    entry,
                    last_used_frame: 0,
                },
            );
        }
    }

    fn evict_if_needed(&self, frame: u64) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() <= self.config.cache_capacity {
            return;
        }
        let mut by_age: Vec<(Tile, u64)> =
            cache.iter().map(|(t, s)| (*t, s.last_used_frame)).collect();
        by_age.sort_by_key(|(_, last)| *last);
        let overflow = cache.len() - self.config.cache_capacity;
        for (tile, _) in by_age.into_iter().take(overflow) {
            // never evict something touched this very frame
            if cache.get(&tile).map(|s| s.last_used_frame) != Some(frame) {
                cache.remove(&tile);
            }
        }
    }
}
