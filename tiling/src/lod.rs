//! level-of-detail selection over a tiling scheme's quadtree.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender},
    },
};

use convection_types::{Camera, Frustum, Globe};
use typed_builder::TypedBuilder;

use crate::{
    factory::TileFactory,
    geometry::TilePatch,
    tile::{Tile, TileImage, TilingScheme},
};

/// how long a failed load is left alone before it is tried again, in frames
const RETRY_AFTER_FRAMES: u64 = 120;
/// how often a tile is loaded before it is given up on for good
const MAX_LOAD_ATTEMPTS: u32 = 3;

#[derive(TypedBuilder, Clone, Copy, Debug)]
pub struct LodConfig {
    /// how far a drawn tile's texels may be stretched on screen before the
    /// tile is refined, in pixels. `1.0` asks for one texel per pixel, larger
    /// values trade sharpness for fewer tiles
    pub target_pixel_error: f64,
    /// coarsest zoom
    pub min_zoom: u32,
    /// finest zoom
    pub max_zoom: u32,
    pub max_concurrent_loads: usize,
    pub cache_capacity: usize,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            target_pixel_error: 1.0,
            min_zoom: 0,
            max_zoom: 18,
            max_concurrent_loads: 8,
            cache_capacity: 512,
        }
    }
}

/// what the tile cache can offer for a tile right now
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileState {
    /// not in the cache, no load started
    Missing,
    /// a load is in flight
    Loading,
    Ready,
    /// the last load failed. it may be retried later
    Failed,
}

impl TileState {
    fn is_drawable(self) -> bool {
        // `Failed` is included because there can be an ancestor below it
        matches!(self, TileState::Ready | TileState::Failed)
    }
}

/// where a drawn tile gets its pixels from
pub enum LodTileSource {
    Exact(Arc<TileImage>),
    /// a coarser ancestor's image, cropped by `uv_rect`
    Fallback {
        image: Arc<TileImage>,
        uv_rect: (f32, f32, f32, f32),
    },
    /// nothing to draw with yet
    None,
}

pub struct LodTile {
    pub tile: Tile,
    pub source: LodTileSource,
    pub patch: TilePatch,
}

/// the outcome of one quadtree walk
struct Selection {
    drawn: Vec<(Tile, TilePatch)>,
    /// wanted tiles with the paired screen space error
    wanted: Vec<(Tile, f64)>,
}

struct Walk<'a, S: TilingScheme> {
    scheme: &'a S,
    globe: &'a Globe,
    camera: &'a Camera,
    frustum: Frustum,
    config: &'a LodConfig,
    state: &'a dyn Fn(Tile) -> TileState,
    selection: Selection,
}

impl<'a, S: TilingScheme> Walk<'a, S> {
    fn is_visible(&self, patch: &TilePatch) -> bool {
        patch.is_visible_from(self.globe, self.camera.position)
            && self
                .frustum
                .intersects_sphere(patch.center(), patch.radius())
    }

    /// projected size of one of the tile image's texels (in pixels)
    fn screen_space_error(&self, patch: &TilePatch) -> f64 {
        let texel = patch.extent().as_meters() / self.scheme.tile_size().max(1) as f64;
        let distance = patch.distance_from(self.globe, self.camera.position);
        self.camera.projected_px(texel, distance)
    }

    fn children(&self, tile: Tile) -> (usize, Vec<(Tile, TilePatch)>) {
        let existing: Vec<Tile> = tile
            .children()
            .into_iter()
            .filter(|child| child.is_valid_for(self.scheme))
            .collect();
        let visible = existing
            .iter()
            .map(|child| (*child, TilePatch::new(self.scheme, *child, self.globe)))
            .filter(|(_, patch)| self.is_visible(patch))
            .collect();
        (existing.len(), visible)
    }

    fn draw(&mut self, tile: Tile, patch: TilePatch) {
        self.selection.drawn.push((tile, patch));
    }

    fn want(&mut self, tile: Tile, error: f64) {
        self.selection.wanted.push((tile, error));
    }

    fn state(&self, tile: Tile) -> TileState {
        (self.state)(tile)
    }

    fn visit(&mut self, tile: Tile, patch: TilePatch) {
        // fall through to children if zoom is smaller than min_zoom
        if tile.zoom() < self.config.min_zoom {
            for (child, child_patch) in self.children(tile).1 {
                self.visit(child, child_patch);
            }
            return;
        }

        let error = self.screen_space_error(&patch);
        self.want(tile, error);

        if tile.zoom() >= self.config.max_zoom || error <= self.config.target_pixel_error {
            self.draw(tile, patch);
            return;
        }

        let (existing, children) = self.children(tile);
        if existing == 0 {
            // if the tile has no children, just draw this tile (even though the
            // screen space error wants more detail)
            self.draw(tile, patch);
            return;
        }
        if children.is_empty() {
            // only the tile's bounding sphere reached into the view, not the
            // footprint itself
            return;
        }

        // refining into a child with nothing to draw would leave a hole, so
        // the children are loaded first and this tile covers for them until
        // they arrive. this is what makes loading proceed coarse to fine
        if children
            .iter()
            .all(|(child, _)| self.state(*child).is_drawable())
        {
            for (child, child_patch) in children {
                self.visit(child, child_patch);
            }
        } else {
            // the children have not been visited, so ask for them
            for (child, child_patch) in &children {
                let child_error = self.screen_space_error(child_patch);
                self.want(*child, child_error);
            }
            self.draw(tile, patch);
        }
    }
}

/// walk the scheme's quadtree and decide what to draw and what to load
fn select_tiles<S: TilingScheme>(
    scheme: &S,
    globe: &Globe,
    camera: &Camera,
    config: &LodConfig,
    state: &dyn Fn(Tile) -> TileState,
) -> Selection {
    let (near, far) = camera.depth_range(globe);
    let mut walk = Walk {
        scheme,
        globe,
        camera,
        frustum: camera.frustum(near, far),
        config,
        state,
        selection: Selection {
            drawn: Vec::new(),
            wanted: Vec::new(),
        },
    };

    for root in scheme.root_tiles() {
        let patch = TilePatch::new(scheme, root, globe);
        if walk.is_visible(&patch) {
            walk.visit(root, patch);
        }
    }

    walk.selection
}

enum CacheEntry {
    Loading { attempt: u32 },
    Ready(Arc<TileImage>),
    Failed { attempts: u32, since_frame: u64 },
}

struct CacheSlot {
    entry: CacheEntry,
    last_used_frame: u64,
}

#[derive(Default)]
struct Cache {
    slots: HashMap<Tile, CacheSlot>,
    inflight: usize,
}

impl Cache {
    fn state(&self, tile: Tile) -> TileState {
        match self.slots.get(&tile).map(|slot| &slot.entry) {
            None => TileState::Missing,
            Some(CacheEntry::Loading { .. }) => TileState::Loading,
            Some(CacheEntry::Ready(_)) => TileState::Ready,
            Some(CacheEntry::Failed { .. }) => TileState::Failed,
        }
    }

    fn ready_image(&self, tile: Tile) -> Option<&Arc<TileImage>> {
        match self.slots.get(&tile) {
            Some(CacheSlot {
                entry: CacheEntry::Ready(image),
                ..
            }) => Some(image),
            _ => None,
        }
    }

    fn touch(&mut self, tile: Tile, frame: u64) {
        if let Some(slot) = self.slots.get_mut(&tile) {
            slot.last_used_frame = frame;
        }
    }
}

/// counters describing the last [`LodTileManager::update`]
#[derive(Clone, Copy, Debug, Default)]
pub struct LodStats {
    pub drawn: usize,
    /// tiles drawn with a coarser ancestor's image
    pub fallbacks: usize,
    pub loading: usize,
    pub cached: usize,
}

pub struct LodTileManager<F: TileFactory + Send + Sync + 'static> {
    factory: Arc<F>,
    config: LodConfig,
    cache: Mutex<Cache>,
    frame: AtomicU64,
    stats: Mutex<LodStats>,
    result_tx: Sender<(Tile, Option<Arc<TileImage>>)>,
    result_rx: Mutex<Receiver<(Tile, Option<Arc<TileImage>>)>>,
}

impl<F: TileFactory + Send + Sync + 'static> LodTileManager<F> {
    pub fn new(factory: F, config: LodConfig) -> Self {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        Self {
            factory: Arc::new(factory),
            config,
            cache: Mutex::new(Cache::default()),
            frame: AtomicU64::new(0),
            stats: Mutex::new(LodStats::default()),
            result_tx,
            result_rx: Mutex::new(result_rx),
        }
    }

    pub fn config(&self) -> &LodConfig {
        &self.config
    }

    pub fn scheme(&self) -> &F::Scheme {
        self.factory.scheme()
    }

    pub fn stats(&self) -> LodStats {
        *self.stats.lock().unwrap()
    }

    /// pick the tiles to draw this frame, and start loading what is missing
    pub fn update(&self, camera: &Camera, globe: &Globe) -> Vec<LodTile> {
        let frame = self.frame.fetch_add(1, Ordering::Relaxed) + 1;

        let mut cache = self.cache.lock().unwrap();
        self.collect_finished_loads(&mut cache, frame);

        let selection = {
            let cache = &*cache;
            let state = |tile: Tile| cache.state(tile);
            select_tiles(self.factory.scheme(), globe, camera, &self.config, &state)
        };

        let drawn: Vec<LodTile> = selection
            .drawn
            .into_iter()
            .map(|(tile, patch)| LodTile {
                tile,
                source: resolve_source(&mut cache, tile, frame),
                patch,
            })
            .collect();

        // touch all wanted tiles, not just the drawn ones
        for (tile, _) in &selection.wanted {
            cache.touch(*tile, frame);
        }

        evict(&mut cache, self.config.cache_capacity, frame);
        let to_load = self.admit_loads(&mut cache, selection.wanted, frame);

        *self.stats.lock().unwrap() = LodStats {
            drawn: drawn.len(),
            fallbacks: drawn
                .iter()
                .filter(|t| matches!(t.source, LodTileSource::Fallback { .. }))
                .count(),
            loading: cache.inflight,
            cached: cache.slots.len(),
        };
        drop(cache);

        for tile in to_load {
            self.spawn_load(tile);
        }
        drawn
    }

    /// start loading as many of the wanted tiles as there are free load slots
    fn admit_loads(
        &self,
        cache: &mut Cache,
        mut wanted: Vec<(Tile, f64)>,
        frame: u64,
    ) -> Vec<Tile> {
        wanted.sort_by(|(_, a), (_, b)| b.total_cmp(a));

        let mut admitted = Vec::new();
        for (tile, _) in wanted {
            if cache.inflight >= self.config.max_concurrent_loads {
                break;
            }
            let attempt = match cache.slots.get(&tile).map(|slot| &slot.entry) {
                None => 1,
                Some(CacheEntry::Failed {
                    attempts,
                    since_frame,
                }) if *attempts < MAX_LOAD_ATTEMPTS
                    && frame.saturating_sub(*since_frame) >= RETRY_AFTER_FRAMES =>
                {
                    attempts + 1
                }
                // already loading, already loaded, or given up on
                Some(_) => continue,
            };

            cache.slots.insert(
                tile,
                CacheSlot {
                    entry: CacheEntry::Loading { attempt },
                    last_used_frame: frame,
                },
            );
            cache.inflight += 1;
            admitted.push(tile);
        }
        admitted
    }

    fn spawn_load(&self, tile: Tile) {
        let factory = self.factory.clone();
        let tx = self.result_tx.clone();
        // FIXME: if removing max_concurrent_loads in the future, use a dynamic
        // thread pool instead
        std::thread::spawn(move || {
            let image = factory.rendered_tile(tile).ok().map(Arc::new);
            let _ = tx.send((tile, image));
        });
    }

    fn collect_finished_loads(&self, cache: &mut Cache, frame: u64) {
        let rx = self.result_rx.lock().unwrap();
        while let Ok((tile, image)) = rx.try_recv() {
            cache.inflight = cache.inflight.saturating_sub(1);
            let attempts = match cache.slots.get(&tile).map(|slot| &slot.entry) {
                Some(CacheEntry::Loading { attempt }) => *attempt,
                _ => 1,
            };
            let entry = match image {
                Some(image) => CacheEntry::Ready(image),
                None => CacheEntry::Failed {
                    attempts,
                    since_frame: frame,
                },
            };
            cache.slots.insert(
                tile,
                CacheSlot {
                    entry,
                    last_used_frame: frame,
                },
            );
        }
    }
}

/// the tile's own image if it is loaded, otherwise a crop of the closest
/// ancestor
fn resolve_source(cache: &mut Cache, tile: Tile, frame: u64) -> LodTileSource {
    if let Some(image) = cache.ready_image(tile).cloned() {
        cache.touch(tile, frame);
        return LodTileSource::Exact(image);
    }

    let mut ancestor = tile;
    while let Some(parent) = ancestor.parent() {
        ancestor = parent;
        let Some(image) = cache.ready_image(ancestor).cloned() else {
            continue;
        };
        let Some((x, y, span)) = tile.offset_in(ancestor) else {
            continue;
        };
        cache.touch(ancestor, frame);
        let span = span as f32;
        return LodTileSource::Fallback {
            image,
            uv_rect: (
                x as f32 / span,
                y as f32 / span,
                (x + 1) as f32 / span,
                (y + 1) as f32 / span,
            ),
        };
    }

    LodTileSource::None
}

/// FILO of drawn tiles once the cache is over capacity. tiles
/// in flight and tiles drawn this frame stay, meaning the cache
/// can temporarily be above capacity to allow large (re)loads
fn evict(cache: &mut Cache, capacity: usize, frame: u64) {
    if cache.slots.len() <= capacity {
        return;
    }

    let mut evictable: Vec<(Tile, u64)> = cache
        .slots
        .iter()
        .filter(|(_, slot)| {
            slot.last_used_frame != frame && !matches!(slot.entry, CacheEntry::Loading { .. })
        })
        .map(|(tile, slot)| (*tile, slot.last_used_frame))
        .collect();
    evictable.sort_by_key(|(_, last_used)| *last_used);

    for (tile, _) in evictable {
        if cache.slots.len() <= capacity {
            break;
        }
        cache.slots.remove(&tile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{factory::DummyTileFactory, tile::WebMercatorScheme};
    use convection_types::Globe;
    use glam::DVec3;
    use image::RgbaImage;

    fn camera_over(globe: &Globe, lon: f64, lat: f64, altitude_m: f64) -> Camera {
        let surface = globe.lonlat_to_point(lon, lat);
        let position = surface.normalize() * (globe.radius.as_meters() + altitude_m);
        Camera::builder()
            .position(position)
            .target(DVec3::ZERO)
            .fov_y_rad(45.0_f64.to_radians())
            .viewport_width_px(1200)
            .viewport_height_px(800)
            .build()
    }

    fn config() -> LodConfig {
        LodConfig {
            max_zoom: 14,
            ..Default::default()
        }
    }

    /// select with a cache that holds every tile `ready` accepts and knows
    /// nothing about the rest
    fn select(camera: &Camera, config: &LodConfig, ready: impl Fn(Tile) -> bool) -> Selection {
        select_with(camera, config, |tile| {
            if ready(tile) {
                TileState::Ready
            } else {
                TileState::Missing
            }
        })
    }

    fn select_with(
        camera: &Camera,
        config: &LodConfig,
        state: impl Fn(Tile) -> TileState,
    ) -> Selection {
        let state: &dyn Fn(Tile) -> TileState = &state;
        select_tiles(
            &WebMercatorScheme::default(),
            &Globe::earth(),
            camera,
            config,
            state,
        )
    }

    fn drawn_tiles(selection: &Selection) -> Vec<Tile> {
        selection.drawn.iter().map(|(tile, _)| *tile).collect()
    }

    fn wanted_tiles(selection: &Selection) -> Vec<Tile> {
        selection.wanted.iter().map(|(tile, _)| *tile).collect()
    }

    #[test]
    fn nothing_loaded_draws_the_root_and_asks_for_its_children() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 20_000_000.0);
        let selection = select(&camera, &config(), |_| false);

        let root = Tile::new(0, 0, 0);
        assert_eq!(drawn_tiles(&selection), vec![root]);

        // the root itself, plus the children it is standing in for
        let wanted = wanted_tiles(&selection);
        assert!(wanted.contains(&root));
        for child in root.children() {
            assert!(wanted.contains(&child), "missing {child:?}");
        }
    }

    #[test]
    fn coarse_tiles_are_requested_before_fine_ones() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 500_000.0);
        let selection = select(&camera, &config(), |_| false);

        let root = selection
            .wanted
            .iter()
            .find(|(tile, _)| tile.zoom() == 0)
            .expect("root wanted");
        for (tile, error) in &selection.wanted {
            if tile.zoom() > 0 {
                assert!(*error < root.1, "{tile:?} outranks the root");
            }
        }
    }

    #[test]
    fn refinement_follows_the_camera_down() {
        let globe = Globe::earth();
        let config = config();
        let mut previous = 0;
        for altitude in [20_000_000.0, 2_000_000.0, 200_000.0, 20_000.0, 2_000.0] {
            let camera = camera_over(&globe, 11.42, 47.27, altitude);
            let selection = select(&camera, &config, |_| true);
            let finest = drawn_tiles(&selection)
                .iter()
                .map(Tile::zoom)
                .max()
                .expect("something is drawn");
            assert!(
                finest > previous,
                "{altitude} m: zoom {finest} did not improve on {previous}"
            );
            previous = finest;
        }
        assert_eq!(previous, config.max_zoom);
    }

    #[test]
    fn drawn_tiles_never_overlap() {
        let globe = Globe::earth();
        for altitude in [20_000_000.0, 1_000_000.0, 10_000.0] {
            let camera = camera_over(&globe, -122.4, 37.8, altitude);
            let tiles = drawn_tiles(&select(&camera, &config(), |_| true));
            for tile in &tiles {
                let mut ancestor = *tile;
                while let Some(parent) = ancestor.parent() {
                    ancestor = parent;
                    assert!(
                        !tiles.contains(&ancestor),
                        "{tile:?} is covered by {ancestor:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn tiles_behind_the_globe_are_not_drawn() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 5_000_000.0);
        let selection = select(&camera, &config(), |_| true);

        for (tile, patch) in &selection.drawn {
            assert!(
                patch.is_visible_from(&globe, camera.position),
                "{tile:?} is beyond the horizon"
            );
        }
        // the far side of the globe is a whole hemisphere of tiles
        let scheme = WebMercatorScheme::default();
        let antipode = scheme.tile_for_lonlat(180.0, 0.0, 4);
        assert!(!drawn_tiles(&selection).contains(&antipode));
    }

    #[test]
    fn a_close_camera_only_draws_what_it_looks_at() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 11.42, 47.27, 10_000.0);
        let selection = select(&camera, &config(), |_| true);
        let tiles = drawn_tiles(&selection);

        // a 45 degree fov 10 km up sees roughly 12 by 8 km of ground, which
        // one texel per pixel resolves into a few dozen tiles
        assert!(
            (32..96).contains(&tiles.len()),
            "{} tiles drawn",
            tiles.len()
        );

        // the ground is all about the same distance away, so the view holds
        // one zoom level, or two where it crosses the error threshold
        let coarsest = tiles.iter().map(Tile::zoom).min().unwrap();
        let finest = tiles.iter().map(Tile::zoom).max().unwrap();
        assert!(finest - coarsest <= 1, "zoom {coarsest} to {finest}");

        // and what the camera points at is covered
        let scheme = WebMercatorScheme::default();
        let center = scheme.tile_for_lonlat(11.42, 47.27, finest);
        assert!(
            tiles
                .iter()
                .any(|tile| center.offset_in(*tile).is_some() || *tile == center),
            "nothing covers the view center"
        );
    }

    #[test]
    fn zoom_range_is_respected() {
        let globe = Globe::earth();
        let config = LodConfig {
            min_zoom: 3,
            max_zoom: 6,
            ..Default::default()
        };
        let camera = camera_over(&globe, 0.0, 0.0, 100_000.0);
        let tiles = drawn_tiles(&select(&camera, &config, |_| true));

        assert!(!tiles.is_empty());
        for tile in tiles {
            assert!((3..=6).contains(&tile.zoom()), "{tile:?} out of range");
        }
    }

    #[test]
    fn a_looser_error_target_draws_fewer_tiles() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 1_000_000.0);
        let sharp = drawn_tiles(&select(&camera, &config(), |_| true)).len();
        let loose = drawn_tiles(&select(
            &camera,
            &LodConfig {
                target_pixel_error: 8.0,
                ..config()
            },
            |_| true,
        ))
        .len();
        assert!(loose < sharp, "{loose} tiles is not fewer than {sharp}");
    }

    #[test]
    fn a_partly_loaded_pyramid_refines_only_where_it_can() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 3_000_000.0);
        // only the top two levels have arrived
        let selection = select(&camera, &config(), |tile| tile.zoom() <= 1);

        assert!(!selection.drawn.is_empty());
        for (tile, _) in &selection.drawn {
            assert!(tile.zoom() <= 2, "{tile:?} was refined into a hole");
        }
        assert!(
            selection.wanted.iter().any(|(tile, _)| tile.zoom() == 2),
            "the next level down was never asked for"
        );
    }

    fn ready_slot(tile: Tile) -> CacheSlot {
        CacheSlot {
            entry: CacheEntry::Ready(Arc::new(TileImage::new(tile, RgbaImage::new(4, 4)))),
            last_used_frame: 0,
        }
    }

    #[test]
    fn a_loaded_tile_is_its_own_source() {
        let tile = Tile::new(2, 1, 1);
        let mut cache = Cache::default();
        cache.slots.insert(tile, ready_slot(tile));

        match resolve_source(&mut cache, tile, 7) {
            LodTileSource::Exact(image) => assert_eq!(image.tile(), tile),
            _ => panic!("expected the tile's own image"),
        }
        assert_eq!(cache.slots[&tile].last_used_frame, 7);
    }

    #[test]
    fn a_missing_tile_borrows_a_crop_of_its_closest_ancestor() {
        let ancestor = Tile::new(2, 1, 1);
        let mut cache = Cache::default();
        cache.slots.insert(ancestor, ready_slot(ancestor));

        // two levels down, so the ancestor's image covers 4x4 of these tiles
        for (tile, expected) in [
            (Tile::new(4, 4, 4), (0.0, 0.0, 0.25, 0.25)),
            (Tile::new(4, 7, 7), (0.75, 0.75, 1.0, 1.0)),
            (Tile::new(4, 5, 6), (0.25, 0.5, 0.5, 0.75)),
        ] {
            match resolve_source(&mut cache, tile, 7) {
                LodTileSource::Fallback { image, uv_rect } => {
                    assert_eq!(image.tile(), ancestor);
                    assert_eq!(uv_rect, expected, "{tile:?}");
                }
                _ => panic!("expected a fallback for {tile:?}"),
            }
        }
        // the ancestor counts as in use, so eviction leaves it alone
        assert_eq!(cache.slots[&ancestor].last_used_frame, 7);

        // a nearer ancestor wins over a further one
        let parent = Tile::new(3, 2, 2);
        cache.slots.insert(parent, ready_slot(parent));
        match resolve_source(&mut cache, Tile::new(4, 4, 4), 8) {
            LodTileSource::Fallback { image, uv_rect } => {
                assert_eq!(image.tile(), parent);
                assert_eq!(uv_rect, (0.0, 0.0, 0.5, 0.5));
            }
            _ => panic!("expected a fallback"),
        }
    }

    #[test]
    fn a_tile_with_no_loaded_ancestor_has_no_source() {
        let mut cache = Cache::default();
        // a sibling's subtree is no help
        let other = Tile::new(3, 0, 0);
        cache.slots.insert(other, ready_slot(other));
        assert!(matches!(
            resolve_source(&mut cache, Tile::new(4, 6, 6), 1),
            LodTileSource::None
        ));
    }

    #[test]
    fn eviction_drops_the_least_recently_drawn() {
        let mut cache = Cache::default();
        for x in 0..8 {
            let tile = Tile::new(4, x, 0);
            let mut slot = ready_slot(tile);
            // the lower the x, the longer ago it was drawn
            slot.last_used_frame = x as u64;
            cache.slots.insert(tile, slot);
        }

        evict(&mut cache, 4, 9);

        assert_eq!(cache.slots.len(), 4);
        for x in 0..8 {
            assert_eq!(
                cache.slots.contains_key(&Tile::new(4, x, 0)),
                x >= 4,
                "tile {x}"
            );
        }
    }

    #[test]
    fn eviction_spares_tiles_in_use_and_in_flight() {
        let mut cache = Cache::default();
        let stale = Tile::new(4, 0, 0);
        cache.slots.insert(stale, ready_slot(stale));

        let loading = Tile::new(4, 1, 0);
        cache.slots.insert(
            loading,
            CacheSlot {
                entry: CacheEntry::Loading { attempt: 1 },
                last_used_frame: 0,
            },
        );
        let in_use = Tile::new(4, 2, 0);
        cache.slots.insert(
            in_use,
            CacheSlot {
                last_used_frame: 7,
                ..ready_slot(in_use)
            },
        );

        evict(&mut cache, 1, 7);

        // only the stale tile can go, so the cache stays above its capacity
        // rather than dropping a tile the frame still needs
        assert!(!cache.slots.contains_key(&stale));
        assert!(cache.slots.contains_key(&loading));
        assert!(cache.slots.contains_key(&in_use));
    }

    #[test]
    fn eviction_leaves_a_cache_within_capacity_alone() {
        let mut cache = Cache::default();
        for x in 0..4 {
            let tile = Tile::new(4, x, 0);
            cache.slots.insert(tile, ready_slot(tile));
        }
        evict(&mut cache, 4, 9);
        assert_eq!(cache.slots.len(), 4);
    }

    #[test]
    fn the_manager_fills_the_pyramid_in_over_frames() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 11.42, 47.27, 2_000_000.0);
        let manager = LodTileManager::new(
            DummyTileFactory::new(),
            LodConfig {
                max_zoom: 5,
                ..Default::default()
            },
        );

        // the first frame has nothing loaded to draw with at all
        let first = manager.update(&camera, &globe);
        assert!(!first.is_empty());
        assert!(
            first
                .iter()
                .all(|drawn| matches!(drawn.source, LodTileSource::None))
        );

        // background loads land over the following frames, one level at a time
        let mut deepest_per_frame = Vec::new();
        let mut settled = Vec::new();
        for _ in 0..600 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let drawn = manager.update(&camera, &globe);
            deepest_per_frame.push(drawn.iter().map(|drawn| drawn.tile.zoom()).max());
            if manager.stats().loading == 0
                && drawn
                    .iter()
                    .all(|drawn| matches!(drawn.source, LodTileSource::Exact(_)))
            {
                settled = drawn;
                break;
            }
        }

        assert!(!settled.is_empty(), "the view never settled");
        assert!(
            deepest_per_frame.windows(2).all(|w| w[0] <= w[1]),
            "refinement went backwards: {deepest_per_frame:?}"
        );
        assert_eq!(
            settled.iter().map(|drawn| drawn.tile.zoom()).max(),
            Some(5),
            "refinement stopped short of the deepest level"
        );
        for drawn in &settled {
            match &drawn.source {
                LodTileSource::Exact(image) => assert_eq!(image.tile(), drawn.tile),
                _ => panic!("{:?} settled without its own image", drawn.tile),
            }
        }
    }

    /// a source with gaps in its coverage, as a real one has at deep zooms
    struct HoleyTileFactory {
        scheme: WebMercatorScheme,
    }

    impl TileFactory for HoleyTileFactory {
        type Scheme = WebMercatorScheme;

        fn new() -> Self {
            Self::with_scheme(WebMercatorScheme::default())
        }

        fn with_scheme(scheme: Self::Scheme) -> Self {
            Self { scheme }
        }

        fn scheme(&self) -> &Self::Scheme {
            &self.scheme
        }

        fn rendered_tile(&self, tile: Tile) -> anyhow::Result<TileImage> {
            anyhow::ensure!(tile.zoom() < 3 || tile.x().is_multiple_of(2), "no coverage");
            let size = self.scheme.tile_size();
            Ok(TileImage::new(tile, RgbaImage::new(size, size)))
        }
    }

    #[test]
    fn a_gap_in_the_source_does_not_hold_back_its_neighbours() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 11.42, 47.27, 2_000_000.0);
        let manager = LodTileManager::new(
            HoleyTileFactory::new(),
            LodConfig {
                max_zoom: 5,
                ..Default::default()
            },
        );

        let mut settled = Vec::new();
        for _ in 0..600 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let drawn = manager.update(&camera, &globe);
            if manager.stats().loading == 0
                && drawn
                    .iter()
                    .all(|drawn| !matches!(drawn.source, LodTileSource::None))
            {
                settled = drawn;
                break;
            }
        }
        assert!(!settled.is_empty(), "the view never settled");

        // refinement reaches the deepest level despite half the tiles missing
        assert_eq!(settled.iter().map(|drawn| drawn.tile.zoom()).max(), Some(5));
        for drawn in &settled {
            match &drawn.source {
                // a tile the source has no image for borrows an ancestor's
                LodTileSource::Fallback { image, uv_rect } => {
                    assert!(drawn.tile.zoom() >= 3 && !drawn.tile.x().is_multiple_of(2));
                    assert!(drawn.tile.offset_in(image.tile()).is_some());
                    assert!(uv_rect.0 < uv_rect.2 && uv_rect.1 < uv_rect.3);
                }
                LodTileSource::Exact(image) => assert_eq!(image.tile(), drawn.tile),
                LodTileSource::None => panic!("{:?} has nothing to draw", drawn.tile),
            }
        }
        assert!(
            settled
                .iter()
                .any(|drawn| matches!(drawn.source, LodTileSource::Fallback { .. })),
            "no tile stood in for a missing one"
        );
    }

    #[test]
    fn a_settled_view_does_not_oscillate_between_levels() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 11.42, 47.27, 40_000.0);
        let manager = LodTileManager::new(
            DummyTileFactory::new(),
            LodConfig {
                max_zoom: 12,
                // deliberately too small to hold the view and the pyramid
                // over it, so eviction runs every frame
                cache_capacity: 16,
                ..Default::default()
            },
        );

        let mut deepest = Vec::new();
        for _ in 0..400 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let drawn = manager.update(&camera, &globe);
            deepest.push(drawn.iter().map(|drawn| drawn.tile.zoom()).max().unwrap());
        }

        // once the view has settled it stays settled: dropping a level to
        // reload an evicted ancestor would show up as the deepest zoom
        // dipping back down
        let settled = &deepest[deepest.len() / 2..];
        assert!(
            settled.iter().all(|zoom| *zoom == 12),
            "levels kept changing: {settled:?}"
        );
    }

    #[test]
    fn the_manager_keeps_loads_within_the_configured_limit() {
        let globe = Globe::earth();
        let camera = camera_over(&globe, 0.0, 0.0, 100_000.0);
        let manager = LodTileManager::new(
            DummyTileFactory::new(),
            LodConfig {
                max_concurrent_loads: 4,
                max_zoom: 8,
                ..Default::default()
            },
        );
        for _ in 0..20 {
            manager.update(&camera, &globe);
            assert!(manager.stats().loading <= 4, "{:?}", manager.stats());
        }
    }
}
