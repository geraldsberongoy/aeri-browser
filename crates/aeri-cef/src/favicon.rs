//! Site icons: the page announces its icon URLs, the browser host downloads
//! the best one and the core receives its pixels as an [`Event::FaviconChanged`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aeri_core::engine::{Event, Favicon};
use aeri_core::ids::TabId;
use cef::rc::Rc as _;
use cef::{
    AlphaType, BrowserHost, CefString, ColorType, DownloadImageCallback, Image, ImplBinaryValue,
    ImplBrowserHost, ImplDownloadImageCallback, ImplImage, WrapDownloadImageCallback,
    wrap_download_image_callback,
};

use crate::{Signal, SignalSink};

/// Twice the sidebar's largest icon (16 pt), so a Retina screen stays sharp.
const MAX_IMAGE_SIZE: u32 = 64;

/// Each tab's latest icon request: a download for an older one (the page
/// named new icons meanwhile) is dropped when it lands.
static TURNS: Mutex<Option<HashMap<TabId, u64>>> = Mutex::new(None);

fn next_turn(tab: TabId) -> u64 {
    let mut turns = TURNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let turn = turns
        .get_or_insert_with(HashMap::new)
        .entry(tab)
        .or_default();
    *turn += 1;
    *turn
}

fn is_current(tab: TabId, turn: u64) -> bool {
    let turns = TURNS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    turns.as_ref().and_then(|turns| turns.get(&tab)) == Some(&turn)
}

/// The page named these icons (best first): fetch the first that downloads.
/// An empty list, or none downloading, means the page has none.
pub(crate) fn changed(
    host: BrowserHost,
    tab: TabId,
    sink: SignalSink,
    page_url: String,
    urls: Vec<String>,
) {
    let turn = next_turn(tab);
    if urls.is_empty() {
        sink(Signal::Engine(Event::FaviconChanged {
            tab,
            page_url,
            favicon: None,
        }));
        return;
    }
    fetch(host, tab, turn, sink, page_url, urls);
}

fn fetch(
    host: BrowserHost,
    tab: TabId,
    turn: u64,
    sink: SignalSink,
    page_url: String,
    mut urls: Vec<String>,
) {
    let url = urls.remove(0);
    let mut callback = IconDownload::new(host.clone(), tab, turn, sink, page_url, urls);
    host.download_image(
        Some(&CefString::from(url.as_str())),
        1,
        MAX_IMAGE_SIZE,
        0,
        Some(&mut callback),
    );
}

wrap_download_image_callback! {
    struct IconDownload {
        host: BrowserHost,
        tab: TabId,
        turn: u64,
        sink: SignalSink,
        page_url: String,
        fallbacks: Vec<String>,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            _http_status_code: ::std::os::raw::c_int,
            image: Option<&mut Image>,
        ) {
            if !is_current(self.tab, self.turn) {
                return;
            }
            let favicon = image.and_then(bitmap);
            if favicon.is_none() && !self.fallbacks.is_empty() {
                fetch(
                    self.host.clone(),
                    self.tab,
                    self.turn,
                    self.sink.clone(),
                    self.page_url.clone(),
                    self.fallbacks.clone(),
                );
                return;
            }
            // None of the page's icons downloaded: it shows its letter.
            (self.sink)(Signal::Engine(Event::FaviconChanged {
                tab: self.tab,
                page_url: self.page_url.clone(),
                favicon,
            }));
        }
    }
}

/// The image's 2x (else nearest) representation as premultiplied BGRA.
fn bitmap(image: &mut Image) -> Option<Favicon> {
    if image.is_empty() != 0 {
        return None;
    }
    let (mut width, mut height) = (0, 0);
    let data = image.as_bitmap(
        2.,
        ColorType::BGRA_8888,
        AlphaType::PREMULTIPLIED,
        Some(&mut width),
        Some(&mut height),
    )?;
    let (width, height) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let mut bgra = vec![0; data.size()];
    if width == 0 || height == 0 || data.data(Some(&mut bgra), 0) != bgra.len() {
        return None;
    }
    if bgra.len() != width as usize * height as usize * 4 {
        return None;
    }
    Some(Favicon {
        width,
        height,
        bgra: Arc::from(bgra),
    })
}
