(function () {
  if (window.__aeriCapture) return;
  var host = null, frame = null, tag = null, band = null, bar = null;
  var live = false, done = null, start = null, dragging = false, target = null;
  // The Mac's accent colour, as "r, g, b" (handed to `on`).
  var accent = '10, 132, 255';

  function css(el, text) { el.style.cssText = text; return el; }

  function build() {
    host = css(document.createElement('div'),
      'all:initial;position:fixed;left:0;top:0;width:0;height:0;z-index:2147483647;pointer-events:none');
    var root = host.attachShadow({ mode: 'closed' });
    // The outline in the accent colour, ringed in white and then a dark
    // hairline so it reads on a page of any colour, a dark one included.
    var look = document.createElement('style');
    look.textContent =
      '.frame{position:fixed;pointer-events:none;box-sizing:border-box;border-radius:4px;display:none;' +
      'border:2px solid rgb(' + accent + ');background:rgba(' + accent + ',.12);' +
      'box-shadow:0 0 0 1px rgba(255,255,255,.9),0 0 0 2px rgba(0,0,0,.35)}' +
      '.band{position:fixed;pointer-events:none;box-sizing:border-box;display:none;' +
      'border:2px solid rgb(' + accent + ');background:rgba(' + accent + ',.08);' +
      'box-shadow:0 0 0 1px rgba(255,255,255,.9),0 0 0 2px rgba(0,0,0,.35),0 0 0 100vmax rgba(0,0,0,.22)}' +
      '@media (prefers-color-scheme: dark){' +
      '.frame{background:rgba(' + accent + ',.24)}' +
      '.band{background:rgba(' + accent + ',.14);' +
      'box-shadow:0 0 0 1px rgba(255,255,255,.9),0 0 0 2px rgba(0,0,0,.5),0 0 0 100vmax rgba(0,0,0,.4)}}';
    root.appendChild(look);
    frame = document.createElement('div');
    frame.className = 'frame';
    tag = css(document.createElement('div'),
      'position:absolute;font:500 11px -apple-system,BlinkMacSystemFont,sans-serif;color:#fff;' +
      'background:rgb(' + accent + ');padding:2px 7px;border-radius:5px;white-space:nowrap;overflow:hidden;' +
      'text-overflow:ellipsis;box-shadow:0 0 0 1px rgba(255,255,255,.6)');
    frame.appendChild(tag);
    // The area being dragged, and everything else a shade darker.
    band = document.createElement('div');
    band.className = 'band';
    bar = css(document.createElement('div'),
      'position:fixed;top:12px;left:50%;transform:translateX(-50%);pointer-events:auto;' +
      'display:flex;align-items:center;gap:10px;padding:5px 5px 5px 12px;border-radius:10px;' +
      'background:rgba(23,23,23,.92);color:rgba(255,255,255,.75);cursor:default;' +
      'font:500 12px -apple-system,BlinkMacSystemFont,sans-serif;white-space:nowrap;' +
      'box-shadow:0 0 0 1px rgba(255,255,255,.16),0 6px 18px rgba(0,0,0,.18)');
    var hint = document.createElement('span');
    hint.textContent = 'Drag an area or click an element';
    var whole = css(document.createElement('div'),
      'padding:4px 10px;border-radius:7px;background:#fff;color:#171717;cursor:pointer');
    whole.textContent = 'Whole Page';
    whole.setAttribute('role', 'button');
    whole.addEventListener('click', function (e) {
      e.preventDefault();
      e.stopPropagation();
      finish(pageChoice('page'));
    });
    var esc = css(document.createElement('div'),
      'padding:4px 8px 4px 2px;color:rgba(255,255,255,.5);cursor:pointer');
    esc.textContent = 'esc';
    esc.addEventListener('click', function (e) { e.stopPropagation(); off(); });
    bar.appendChild(hint);
    bar.appendChild(whole);
    bar.appendChild(esc);
    root.appendChild(band);
    root.appendChild(frame);
    root.appendChild(bar);
    document.documentElement.appendChild(host);
  }

  var known = {
    nav: 'Navigation', header: 'Header', footer: 'Footer', aside: 'Sidebar', main: 'Main',
    article: 'Article', section: 'Section', form: 'Form', dialog: 'Dialog', video: 'Video',
    img: 'Image', picture: 'Image', svg: 'Graphic', canvas: 'Canvas', button: 'Button',
    iframe: 'Embed', figure: 'Figure', table: 'Table', pre: 'Code'
  };

  function name(el) {
    var said = el.getAttribute && (el.getAttribute('aria-label') || el.getAttribute('title'));
    var what = said && said.trim() ? said.trim()
             : (known[el.tagName.toLowerCase()] || el.tagName.toLowerCase());
    if (what.length > 40) what = what.slice(0, 40) + '\u2026';
    var r = el.getBoundingClientRect();
    return what + ' \u00b7 ' + Math.round(r.width) + '\u00d7' + Math.round(r.height);
  }

  function place(el) {
    var r = el.getBoundingClientRect();
    frame.style.display = 'block';
    frame.style.left = r.left + 'px';
    frame.style.top = r.top + 'px';
    frame.style.width = r.width + 'px';
    frame.style.height = r.height + 'px';
    tag.textContent = name(el);
    tag.style.top = r.top >= 26 ? '-21px' : '3px';
    tag.style.left = Math.max(2, -r.left + 4) + 'px';
    tag.style.maxWidth = Math.max(80, window.innerWidth - Math.max(0, r.left) - 16) + 'px';
  }

  function under(x, y) {
    var el = document.elementFromPoint(x, y);
    if (!el || el === host || el === document.documentElement) return null;
    return el;
  }

  // What the page says it chose, in CSS pixels of the viewport, with how
  // far it is scrolled, how big it is, and the pixels a CSS pixel is.
  function pageChoice(kind, r) {
    var doc = document.documentElement, body = document.body;
    return {
      kind: kind,
      x: r ? r.left : 0, y: r ? r.top : 0, w: r ? r.width : 0, h: r ? r.height : 0,
      sx: window.scrollX, sy: window.scrollY,
      dw: Math.max(doc.scrollWidth, body ? body.scrollWidth : 0, doc.clientWidth),
      dh: Math.max(doc.scrollHeight, body ? body.scrollHeight : 0, doc.clientHeight),
      dpr: window.devicePixelRatio || 1
    };
  }

  function ours(e) { return e.target === host; }

  function swallow(e) {
    if (!live || ours(e)) return;
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
  }

  function onDown(e) {
    if (!live || ours(e)) return;
    swallow(e);
    if (e.button !== 0) return;
    start = { x: e.clientX, y: e.clientY };
    dragging = false;
  }

  function onMove(e) {
    if (!live) return;
    if (start) {
      var dx = e.clientX - start.x, dy = e.clientY - start.y;
      if (!dragging && dx * dx + dy * dy < 16) return;
      dragging = true;
      frame.style.display = 'none';
      band.style.display = 'block';
      band.style.left = Math.min(start.x, e.clientX) + 'px';
      band.style.top = Math.min(start.y, e.clientY) + 'px';
      band.style.width = Math.abs(dx) + 'px';
      band.style.height = Math.abs(dy) + 'px';
      return;
    }
    if (ours(e)) { frame.style.display = 'none'; target = null; return; }
    var el = under(e.clientX, e.clientY);
    if (!el) return;
    target = el;
    place(el);
  }

  // What a press brings after its pointerup, a click or a mouseup, is
  // still the page's once the layer is down, eaten for a moment: a link or
  // a button clicked to be captured was otherwise followed or pressed as
  // the picture was taken.
  var after = ['click', 'dblclick', 'mouseup', 'auxclick', 'contextmenu'];

  function eat(e) {
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
  }

  function trail() {
    after.forEach(function (kind) { window.addEventListener(kind, eat, true); });
    setTimeout(function () {
      after.forEach(function (kind) { window.removeEventListener(kind, eat, true); });
    }, 500);
  }

  function onUp(e) {
    if (!live || !start) return;
    swallow(e);
    var from = start;
    start = null;
    if (dragging) {
      dragging = false;
      var r = {
        left: Math.min(from.x, e.clientX), top: Math.min(from.y, e.clientY),
        width: Math.abs(e.clientX - from.x), height: Math.abs(e.clientY - from.y)
      };
      if (r.width < 4 || r.height < 4) { band.style.display = 'none'; return; }
      trail();
      finish(pageChoice('area', r));
      return;
    }
    var el = target || under(e.clientX, e.clientY);
    if (!el) return;
    trail();
    finish(pageChoice('element', el.getBoundingClientRect()));
  }

  // The press became a drag of a link, an image or text and sends no
  // pointerup. A press that wasn't yet an area is the click it was meant as.
  function onCancel() {
    if (!live || !start) return;
    var from = start;
    start = null;
    if (dragging) { dragging = false; band.style.display = 'none'; return; }
    var el = target || under(from.x, from.y);
    if (!el) return;
    trail();
    finish(pageChoice('element', el.getBoundingClientRect()));
  }

  function onKey(e) {
    if (!live || e.key !== 'Escape') return;
    swallow(e);
    off();
  }

  var presses = ['mousedown', 'mouseup', 'click', 'dblclick', 'contextmenu', 'touchstart', 'auxclick',
                 'dragstart', 'selectstart'];

  function listen(add) {
    var how = add ? 'addEventListener' : 'removeEventListener';
    window[how]('pointerdown', onDown, true);
    window[how]('pointermove', onMove, true);
    window[how]('pointerup', onUp, true);
    window[how]('pointercancel', onCancel, true);
    window[how]('keydown', onKey, true);
    presses.forEach(function (kind) { window[how](kind, swallow, true); });
  }

  function teardown() {
    live = false;
    start = null;
    dragging = false;
    target = null;
    listen(false);
    document.documentElement.style.cursor = '';
    if (host && host.parentNode) host.parentNode.removeChild(host);
    host = null;
  }

  function finish(choice) {
    var answer = done;
    done = null;
    teardown();
    if (answer) answer(choice);
  }

  function off() { finish(null); }

  window.__aeriCapture = {
    on: function (answer, colour) {
      if (live) finish(null);
      if (colour) accent = colour;
      done = answer;
      live = true;
      build();
      document.documentElement.style.cursor = 'crosshair';
      listen(true);
    },
    off: off
  };
})();
