//! UI-only images have a bounded lifetime, independent of the keyboard session.
use std::{collections::VecDeque, sync::Arc};

const MAX_TEXTURES: usize = 128;
const MAX_BYTES: usize = 8 * 1024 * 1024;
type SharedCache = Arc<egui::mutex::Mutex<TextureCache>>;

#[derive(Default)]
struct TextureCache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

struct Entry {
    slot: egui::Id,
    revision: egui::Id,
    texture: egui::TextureHandle,
    bytes: usize,
}

/// Reuse a slot when its content changes; evict least-recently-used images.
/// The image closure runs only on a miss, outside egui's context-data lock.
pub(crate) fn texture(
    ctx: &egui::Context,
    slot: impl std::hash::Hash + std::fmt::Debug,
    revision: impl std::hash::Hash + std::fmt::Debug,
    image: impl FnOnce() -> Option<egui::ColorImage>,
) -> Option<egui::TextureHandle> {
    let cache = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<SharedCache>(egui::Id::unique("entropy_ui_textures"))
            .clone()
    });
    let mut cache = cache.lock();
    let slot = egui::Id::unique(slot);
    let revision = egui::Id::unique(revision);
    let previous = cache
        .entries
        .iter()
        .position(|entry| entry.slot == slot)
        .and_then(|index| cache.entries.remove(index));
    if let Some(entry) = previous.as_ref() {
        cache.bytes -= entry.bytes;
    }
    let entry = match previous {
        Some(entry) if entry.revision == revision => entry,
        previous => {
            let image = image()?;
            let bytes = image.pixels.len() * std::mem::size_of::<egui::Color32>();
            let texture = if let Some(mut entry) = previous {
                entry.texture.set(image, egui::TextureOptions::LINEAR);
                entry.texture
            } else {
                ctx.load_texture(
                    format!("entropy-{slot:?}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            };
            Entry {
                slot,
                revision,
                texture,
                bytes,
            }
        }
    };
    if entry.bytes > MAX_BYTES {
        return Some(entry.texture);
    }
    while !cache.entries.is_empty()
        && (cache.entries.len() >= MAX_TEXTURES || cache.bytes + entry.bytes > MAX_BYTES)
    {
        let removed = cache.entries.pop_front().unwrap();
        cache.bytes -= removed.bytes;
    }
    let texture = entry.texture.clone();
    cache.bytes += entry.bytes;
    cache.entries.push_back(entry);
    Some(texture)
}

pub(crate) fn clear(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<SharedCache>(egui::Id::unique("entropy_ui_textures")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_and_releasing_images_frees_texture_allocations() {
        let ctx = egui::Context::default();
        let before = ctx.tex_manager().read().num_allocated();
        for revision in 0..20 {
            drop(
                texture(&ctx, "preview", revision, || {
                    Some(egui::ColorImage::filled([240, 280], egui::Color32::WHITE))
                })
                .unwrap(),
            );
        }
        assert_eq!(ctx.tex_manager().read().num_allocated(), before + 1);
        clear(&ctx);
        assert_eq!(ctx.tex_manager().read().num_allocated(), before);
    }

    #[test]
    fn oversized_images_are_not_retained() {
        let ctx = egui::Context::default();
        let before = ctx.tex_manager().read().num_allocated();
        drop(
            texture(&ctx, "oversized", 0, || {
                Some(egui::ColorImage::filled([2048, 1025], egui::Color32::WHITE))
            })
            .unwrap(),
        );
        assert_eq!(ctx.tex_manager().read().num_allocated(), before);
    }

    #[test]
    fn large_images_evict_by_bytes_and_cached_images_skip_decoding() {
        let ctx = egui::Context::default();
        for slot in 0..10 {
            drop(
                texture(&ctx, slot, 0, || {
                    Some(egui::ColorImage::filled([512, 512], egui::Color32::WHITE))
                })
                .unwrap(),
            );
        }
        let cache = ctx.data(|data| {
            data.get_temp::<SharedCache>(egui::Id::unique("entropy_ui_textures"))
                .unwrap()
        });
        assert_eq!(cache.lock().bytes, MAX_BYTES);
        drop(
            texture(&ctx, 9, 0, || {
                panic!("a cache hit must not decode the image")
            })
            .unwrap(),
        );
    }
}
