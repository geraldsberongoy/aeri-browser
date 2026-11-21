//! Site icons kept on disk, per profile and by host, so a restored or
//! sleeping tab wears its site's icon before its page loads (Search
//! `Icons.swift`: small PNGs next to the history, cleared with it).
//!
//! `Profiles/<id>/icons/<host>.png`. Files are read once, lazily, into an
//! in-memory map of ready-to-draw images; they are written off the main
//! thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use aeri_core::engine::Favicon;
use gpui::{App, RenderImage};

pub(super) struct SiteIcons {
    dir: PathBuf,
    /// `None`: the host has no icon on disk (asked once).
    images: RefCell<HashMap<String, Option<Arc<RenderImage>>>>,
}

impl SiteIcons {
    pub(super) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            images: RefCell::default(),
        }
    }

    /// The host's kept icon, read from disk the first time it is asked for.
    pub(super) fn get(&self, host: &str) -> Option<Arc<RenderImage>> {
        let name = file_name(host)?;
        self.images
            .borrow_mut()
            .entry(host.to_owned())
            .or_insert_with(|| read(&self.dir.join(name)))
            .clone()
    }

    /// An icon arrived for `host`: it replaces the kept one, in memory now
    /// and on disk in the background.
    pub(super) fn put(&self, host: &str, image: Arc<RenderImage>, favicon: &Favicon, cx: &mut App) {
        let Some(name) = file_name(host) else {
            return;
        };
        let old = self
            .images
            .borrow_mut()
            .insert(host.to_owned(), Some(image.clone()));
        if let Some(old) = old.flatten()
            && !Arc::ptr_eq(&old, &image)
        {
            cx.drop_image(old, None);
        }
        let (dir, favicon) = (self.dir.clone(), favicon.clone());
        std::thread::spawn(move || {
            if let Err(err) = write(&dir, &name, &favicon) {
                tracing::warn!(%err, "could not keep a site icon");
            }
        });
    }

    /// Clear History: every kept icon goes.
    pub(super) fn clear(&self, cx: &mut App) {
        for image in self.images.borrow_mut().drain().filter_map(|(_, i)| i) {
            cx.drop_image(image, None);
        }
        let dir = self.dir.clone();
        std::thread::spawn(move || {
            if let Err(err) = std::fs::remove_dir_all(&dir)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%err, "could not remove the kept site icons");
            }
        });
    }
}

/// `<host>.png`, with anything a file name could not hold made safe.
fn file_name(host: &str) -> Option<String> {
    if host.is_empty() {
        return None;
    }
    let safe: String = host
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(format!("{safe}.png"))
}

fn write(dir: &std::path::Path, name: &str, favicon: &Favicon) -> Result<(), image::ImageError> {
    std::fs::create_dir_all(dir)?;
    // CEF's pixels are premultiplied BGRA; PNG holds straight RGBA.
    let mut rgba = favicon.bgra.to_vec();
    for px in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(px[3]);
        if alpha != 0 && alpha != 255 {
            for c in &mut px[..3] {
                *c = (u32::from(*c) * 255 / alpha).min(255) as u8;
            }
        }
        px.swap(0, 2);
    }
    let Some(pixels) = image::RgbaImage::from_raw(favicon.width, favicon.height, rgba) else {
        return Ok(());
    };
    pixels.save_with_format(dir.join(name), image::ImageFormat::Png)
}

fn read(path: &std::path::Path) -> Option<Arc<RenderImage>> {
    let mut pixels = image::open(path).ok()?.into_rgba8();
    // GPUI sprites are premultiplied BGRA.
    for px in pixels.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(px[3]);
        if alpha != 255 {
            for c in &mut px[..3] {
                *c = (u32::from(*c) * alpha / 255) as u8;
            }
        }
        px.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(pixels)])))
}
