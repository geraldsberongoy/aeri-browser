//! The scripts the floating video runs in its page (Search `Isolate` and
//! `NativePiP`, `Float.swift:1145-1537`, `NativePiP.swift`), run through
//! `TabWindow::evaluate` in the page's own world. They are Search's own,
//! with WebKit's calls swapped for Chromium's (the picture in picture
//! API) and what they answer made plain text (`TabWindow::evaluate` gives
//! a string, a boolean or a number).

/// Everything but the video, out of the way. Visibility is inherited, so
/// hiding the body and turning it back on for the video alone leaves the
/// player's own machinery running untouched, which is what keeps the stream
/// alive where cutting the DOM about would kill it (`Isolate.on`,
/// :1175-1347). Answers `floating` or `none`.
pub(super) const ON: &str = r##"(function () {
  if (document.fullscreenElement) return 'none';
  var videos = document.querySelectorAll('video');
  var unseen = window.__officeFloatUnseen || function () { return false; };
  var best = null, area = 0;
  for (var i = 0; i < videos.length; i++) {
    var v = videos[i];
    if ((v.paused && !unseen(v)) || v.ended || v.readyState < 2) continue;
    var box = v.getBoundingClientRect();
    if (box.width * box.height >= area) { area = box.width * box.height; best = v; }
  }
  if (!best) return 'none';

  var r = best.getBoundingClientRect(), vw = best.videoWidth, vh = best.videoHeight;
  // Where the video was in the page, at what size of page, for the landing
  // to wait for (see OFF). Not while the last float is still landing: the
  // page isn't as it was yet, and the place it is going back to is the one
  // kept from before. And whether Twitch's chat was open, for opening it
  // again (see OFF).
  var wait = window.__officeFloatWait;
  if (!window.__officeFloatHome || !(window.__officeFloatLanding || (wait && wait.landing && !wait.settled))) {
    window.__officeFloatHome = {
      box: [r.left + scrollX, r.top + scrollY, r.width, r.height],
      size: [best.offsetWidth, best.offsetHeight],
      wide: innerWidth, high: innerHeight,
      chat: !!document.querySelector('.right-column:not(.right-column--collapsed) [data-a-target="right-column__toggle-collapse-btn"]')
    };
  }
  var size = window.__officeFloatHome.size;
  // And the picture in it, as `contain` fits it. Twitch's player, in its
  // Studio mode, gives the video the page's whole height and draws the
  // picture in the middle of it, between black bars.
  var shown = size;
  if (vw && vh && size[0] && size[1]) {
    var within = Math.min(size[0] / vw, size[1] / vh);
    shown = [vw * within, vh * within];
  }

  // A landing still waiting for its tab is called off: the page is out again.
  window.__officeFloatLanding = null;
  window.__officeFloatWait = null;
  best.setAttribute('data-office-float', '');
  var sheet = document.getElementById('office-float');
  if (!sheet) {
    sheet = document.createElement('style');
    sheet.id = 'office-float';
    (document.head || document.documentElement).appendChild(sheet);
  }
  sheet.textContent = [
    'html.office-floating, html.office-floating body {',
    'background:#000 !important; overflow:hidden !important; margin:0 !important}',
    'html.office-floating body > * { visibility:hidden !important }',
    'html.office-floating [data-office-float] {',
    'visibility:visible !important; position:fixed !important;',
    'left:0 !important; top:0 !important; right:0 !important; bottom:0 !important;',
    'width:100vw !important; height:100vh !important;',
    'max-width:none !important; max-height:none !important;',
    // The player's own box and clipping must not cut off the floating picture.
    'min-width:0 !important; min-height:0 !important; margin:0 !important; padding:0 !important; border:0 !important;',
    'clip-path:none !important; mask:none !important; -webkit-mask:none !important;',
    // Players such as Netflix center the element with a translation. With
    // our top/left at zero, that moves it out of the floating window.
    'transform:none !important; translate:none !important; rotate:none !important; scale:none !important;',
    'opacity:1 !important; object-fit:contain !important; z-index:2147483647 !important}',
    // Better, where the engine can divide one length by another: the video
    // keeps the size it had in its tab, and is only scaled to the window,
    // its picture as large as it goes whole and in the middle.
    size[0] && size[1] ? [
      '@supports (scale: calc(100vw / 1px)) {',
      'html.office-floating [data-office-float] {',
      '--office-float-by: min(calc(100vw / ' + shown[0] + 'px), calc(100vh / ' + shown[1] + 'px));',
      'right:auto !important; bottom:auto !important; margin:0 !important; box-sizing:border-box !important;',
      'width:' + size[0] + 'px !important; height:' + size[1] + 'px !important;',
      'min-width:0 !important; min-height:0 !important; transform-origin:0 0 !important;',
      'transform:translate(calc((100vw - ' + size[0] + 'px * var(--office-float-by)) / 2),',
      ' calc((100vh - ' + size[1] + 'px * var(--office-float-by)) / 2)) scale(var(--office-float-by)) !important}}'
    ].join('') : '',
    // Netflix renders timed text after the video, in a layer of its own
    // beside it or one level up. Keep it above the video without exposing
    // the rest of the player.
    'html.office-floating [data-office-float] ~ .player-timedtext,',
    'html.office-floating :has(> [data-office-float]) > .player-timedtext,',
    'html.office-floating :has([data-office-float]) > .player-timedtext {',
    'visibility:visible !important; z-index:2147483647 !important}',
    // Fixed or not, the video is still cut to the box of any ancestor that
    // clips, and in a window this small that box sits partly or wholly off
    // screen. And fixed is only fixed to the window while no ancestor makes
    // a box of its own for it: a transform, a filter, containment, a
    // perspective, a backdrop, a container query.
    'html.office-floating body :has([data-office-float]) {',
    'overflow:visible !important;',
    'transform:none !important; translate:none !important; rotate:none !important; scale:none !important;',
    'filter:none !important; backdrop-filter:none !important; -webkit-backdrop-filter:none !important;',
    'perspective:none !important; contain:none !important; container-type:normal !important;',
    'will-change:auto !important; content-visibility:visible !important;',
    'clip-path:none !important; mask:none !important; -webkit-mask:none !important;',
    'opacity:1 !important}',
    // The player's own controls would sit under ours.
    'html.office-floating [data-office-float]::-webkit-media-controls {',
    'display:none !important}'
  ].join('');
  document.documentElement.classList.add('office-floating');

  // The mark has to be defended. Everything but the marked element is
  // hidden, so the moment a player rebuilds its DOM (a quality change, an ad
  // break, a React re-render) the mark goes with the old element and the
  // window turns black while holding a perfectly live page. So the mark is
  // put back on whatever is playing now, for as long as the page is out:
  // when a video starts to play, within a quarter of a second of the page's
  // elements changing or the mark being taken off, and every two seconds
  // for a page that does neither.
  if (window.__officeFloatUnwatch) window.__officeFloatUnwatch();
  clearInterval(window.__officeFloatWatch);
  var remark = function () {
    if (document.querySelector('[data-office-float]')) return;
    var again = null, most = 0;
    var all = document.querySelectorAll('video');
    for (var j = 0; j < all.length; j++) {
      var one = all[j];
      if (one.paused || one.ended || one.readyState < 2) continue;
      var shape = one.getBoundingClientRect();
      if (shape.width * shape.height >= most) {
        most = shape.width * shape.height;
        again = one;
      }
    }
    if (again) again.setAttribute('data-office-float', '');
  };
  var queued = null;
  var changes = new MutationObserver(function () {
    if (!queued) queued = setTimeout(function () { queued = null; remark(); }, 250);
  });
  changes.observe(document.documentElement, {
    childList: true, subtree: true, attributes: true, attributeFilter: ['data-office-float']
  });
  // Media events don't bubble: caught on the way down.
  document.addEventListener('playing', remark, true);
  window.__officeFloatWatch = setInterval(remark, 2000);
  window.__officeFloatUnwatch = function () {
    changes.disconnect();
    clearTimeout(queued);
    document.removeEventListener('playing', remark, true);
    window.__officeFloatUnwatch = null;
  };
  return 'floating';
})()"##;

/// Whether the page has settled where it now is: see [`fits`] and [`OFF`],
/// which set what this reads (`Isolate.settled`, :1352).
pub(super) const SETTLED: &str = "!!(window.__officeFloatWait && window.__officeFloatWait.settled)";

/// Out in the floating window: settled once the page is laid out at the
/// window's size, in CSS pixels, with the video alone in it. Checked as
/// each frame is drawn (`Isolate.fits`, :1357).
pub(super) fn fits(width: f64, height: f64) -> String {
    format!(
        r#"(function () {{
  var wait = window.__officeFloatWait = {{ settled: false }}, began = Date.now();
  (function frame() {{
    if (window.__officeFloatWait !== wait || Date.now() - began > 3000) return;
    var on = document.documentElement.classList.contains('office-floating');
    if (on && Math.abs(innerWidth - {width}) <= 2 && Math.abs(innerHeight - {height}) <= 2) {{
      wait.settled = true;
      return;
    }}
    requestAnimationFrame(frame);
  }})();
  return true;
}})()"#
    )
}

/// Step over the bit you missed, or back to it (`Isolate.skip`, :1377).
pub(super) fn skip(seconds: f64) -> String {
    format!(
        r#"(function () {{
  var video = document.querySelector('[data-office-float]')
    || document.querySelector('video');
  if (!video) return false;
  video.currentTime = Math.max(0, video.currentTime + ({seconds}));
  return true;
}})()"#
    )
}

/// How far through, and whether it is running, as `through,playing`
/// (`Isolate.where_`, :1390).
pub(super) const WHERE: &str = r#"(function () {
  var video = document.querySelector('[data-office-float]')
    || document.querySelector('video');
  if (!video || !video.duration || !isFinite(video.duration)) return '0,1';
  return (video.currentTime / video.duration) + ',' + (video.paused ? 0 : 1);
})()"#;

/// Stop or start it, and say which it is now (`Isolate.toggle`, :1399).
pub(super) const TOGGLE: &str = r#"(function () {
  var video = document.querySelector('[data-office-float]')
    || document.querySelector('video');
  if (!video) return true;
  if (video.paused) { video.play(); } else { video.pause(); }
  return !video.paused;
})()"#;

/// Everything back as it was, once the page is back in its tab and laid
/// out at the tab's size. Put back at once, while the page still had the
/// little window's size, the player fitted the video to that, and the tab's
/// first frames could show it so (`Isolate.off`, :1414-1535). The picture
/// in picture the page may have been asked for is left too, since some
/// players ask for their own.
pub(super) const OFF: &str = r#"(function () {
  try {
    var out = document.querySelector('video[data-office-float]')
      || document.querySelector('video');
    if (out && document.pictureInPictureElement && document.exitPictureInPicture) {
      document.exitPictureInPicture();
    }
  } catch (e) {}

  var root = document.documentElement;
  var landing = window.__officeFloatLanding = {};
  var wait = window.__officeFloatWait = { settled: false, landing: true };
  function put() {
    // Floated again in the meantime: that is the float's now.
    if (window.__officeFloatLanding !== landing) return;
    window.__officeFloatLanding = null;
    if (window.__officeFloatUnwatch) window.__officeFloatUnwatch();
    clearInterval(window.__officeFloatWatch);
    window.__officeFloatWatch = null;
    root.classList.remove('office-floating');
    var sheet = document.getElementById('office-float');
    if (sheet) sheet.textContent = '';
    var video = document.querySelector('[data-office-float]');
    if (video) video.removeAttribute('data-office-float');
    settle(video);
    reopen();
    redraw(video);
  }
  // Put back while its tab wasn't in front, the video was sized to the tab
  // again only as the tab came back. Taken out of the layout for a frame
  // once its tab is in front, it is drawn at its own size.
  function redraw(video) {
    if (document.visibilityState !== 'hidden') return;
    document.addEventListener('visibilitychange', function seen() {
      if (document.visibilityState !== 'visible') return;
      document.removeEventListener('visibilitychange', seen);
      requestAnimationFrame(function () {
        requestAnimationFrame(function () {
          if (root.classList.contains('office-floating')) return;
          var v = video && video.isConnected ? video : document.querySelector('video');
          if (!v) return;
          var shown = v.style.display;
          v.style.display = 'none';
          requestAnimationFrame(function () { v.style.display = shown; });
        });
      });
    });
  }
  // Twitch, in its Studio mode, folds its chat away in a page as narrow as
  // the little window, and opens it again only when asked: asked here, if it
  // was open as the video went out, once the page is the tab's size again.
  function reopen() {
    var home = window.__officeFloatHome;
    if (!home || !home.chat || root.classList.contains('office-floating')) return;
    if (innerWidth === wide && innerHeight === high) {
      addEventListener('resize', function again() {
        removeEventListener('resize', again);
        reopen();
      });
      return;
    }
    var shut = document.querySelector('.right-column--collapsed [data-a-target="right-column__toggle-collapse-btn"]');
    if (shut) shut.click();
  }
  // Settled once the video is back where it was, at the size it was, in a
  // page of the size it was, looked at as each frame is drawn. A window
  // resized meanwhile has no such place to go back to, and settles once the
  // video has kept still for 0.3 s.
  function settle(video) {
    var home = window.__officeFloatHome, began = Date.now(), last = null, still = 0;
    function near(a, b) {
      for (var i = 0; i < 4; i++) if (Math.abs(a[i] - b[i]) > 2) return false;
      return true;
    }
    function done() { wait.settled = true; }
    (function frame() {
      if (window.__officeFloatWait !== wait) return;
      var v = video && video.isConnected ? video : document.querySelector('video');
      if (!home || !v || Date.now() - began > 3000) return done();
      var r = v.getBoundingClientRect(), box = [r.left + scrollX, r.top + scrollY, r.width, r.height];
      if (innerWidth === home.wide && innerHeight === home.high) {
        if (near(box, home.box)) return done();
      } else {
        still = last && near(box, last) ? still || Date.now() : 0;
        last = box;
        if (still && Date.now() - still >= 300) return done();
      }
      requestAnimationFrame(frame);
    })();
  }
  // Each frame until the page is laid out at another size than the little
  // window's, the tab's, and its player has had that frame's resize to fit
  // the video to it. A page not drawn is put back by the clock.
  var wide = innerWidth, high = innerHeight, began = Date.now();
  (function frame() {
    if (window.__officeFloatLanding !== landing) return;
    if (innerWidth !== wide || innerHeight !== high || Date.now() - began > 400) return put();
    requestAnimationFrame(frame);
  })();
  setTimeout(put, 1000);
  return 'landing';
})()"#;

/// The system's picture in picture for the page's video (`NativePiP.script`,
/// with Chromium's API): a video already in it leaves it (`manual`) or is
/// left; else the largest playing video goes in, or with `manual` the
/// largest that has a frame. Answers `in`, `out`, `none` or `refused`.
/// Needs a user gesture: the call is made as one.
pub(super) fn pip_on(manual: bool) -> String {
    format!(
        r#"(async function (manual) {{
  try {{
    if (document.pictureInPictureElement) {{
      if (manual) {{ await document.exitPictureInPicture(); return 'out'; }}
      return 'in';
    }}
    var videos = document.querySelectorAll('video');
    var unseen = window.__officeFloatUnseen || function () {{ return false; }};
    var best = null, area = 0, any = null, most = 0;
    for (var i = 0; i < videos.length; i++) {{
      var v = videos[i];
      if (v.ended || v.readyState < 2) continue;
      var box = v.getBoundingClientRect(), size = box.width * box.height;
      if (size >= most) {{ most = size; any = v; }}
      if ((!v.paused || unseen(v)) && size >= area) {{ area = size; best = v; }}
    }}
    best = best || (manual ? any : null);
    if (!best) return 'none';
    if (!document.pictureInPictureEnabled || best.disablePictureInPicture) return 'refused';
    await best.requestPictureInPicture();
    return 'in';
  }} catch (e) {{ return 'refused'; }}
}})({manual})"#
    )
}

/// Back out of picture in picture (`NativePiP.off`).
pub(super) const PIP_OFF: &str = r#"(async function () {
  try {
    if (document.pictureInPictureElement) await document.exitPictureInPicture();
  } catch (e) {}
  return 'out';
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// A script with unbalanced braces or parentheses would be a syntax
    /// error in the page, which none of the Rust checks would see.
    fn balanced(script: &str) -> bool {
        let mut stack = Vec::new();
        let mut quote = None;
        let mut chars = script.chars().peekable();
        while let Some(c) = chars.next() {
            if let Some(q) = quote {
                if c == '\\' {
                    chars.next();
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '\'' | '"' => quote = Some(c),
                '/' if chars.peek() == Some(&'/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                '(' | '{' | '[' => stack.push(c),
                ')' | '}' | ']' => {
                    let open = match c {
                        ')' => '(',
                        '}' => '{',
                        _ => '[',
                    };
                    if stack.pop() != Some(open) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        stack.is_empty() && quote.is_none()
    }

    #[test]
    fn every_script_is_balanced() {
        for script in [
            ON,
            OFF,
            WHERE,
            TOGGLE,
            PIP_OFF,
            SETTLED,
            &fits(440., 247.),
            &skip(-15.),
            &pip_on(true),
            &pip_on(false),
        ] {
            assert!(balanced(script), "{script}");
        }
    }

    #[test]
    fn the_numbers_go_in() {
        assert!(fits(440.5, 247.).contains("innerWidth - 440.5"));
        assert!(skip(-15.).contains("(-15)"));
        assert!(pip_on(true).ends_with("(true)"));
    }
}
