// The article's own light and dark, in its corner while reading mode is up
// (Search ReaderLook, Reader.swift:582-621): three buttons in a capsule, 24 by
// 22, quiet (0.55) until the pointer comes to it, in the article's colours.
// It lives in the page because the chrome cannot draw over a page's window;
// its closed shadow keeps the page's own styles out. A click puts the look on
// <html> and tells the browser, which keeps it for the next article.
(function () {
  var old = document.getElementById('office-reader-look');
  if (old) old.remove();
  var root = document.documentElement;
  var host = document.createElement('div');
  host.id = 'office-reader-look';
  host.style.cssText = 'all:initial;position:fixed;top:14px;right:14px;z-index:2147483647;';
  var shadow = host.attachShadow({ mode: 'closed' });
  var icons = {
    light: '<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"><circle cx="8" cy="8" r="2.9"/><path d="M8 1.2v1.6M8 13.2v1.6M1.2 8h1.6M13.2 8h1.6M3.2 3.2l1.1 1.1M11.7 11.7l1.1 1.1M3.2 12.8l1.1-1.1M11.7 4.3l1.1-1.1"/></svg>',
    dark: '<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"><path d="M13.4 9.6A5.6 5.6 0 0 1 6.4 2.6a5.6 5.6 0 1 0 7 7z"/></svg>',
    system: '<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3"><circle cx="8" cy="8" r="5.7"/><path d="M8 2.3a5.7 5.7 0 0 0 0 11.4z" fill="currentColor" stroke="none"/></svg>'
  };
  var titles = { light: 'Light', dark: 'Dark', system: 'As the window is' };
  shadow.innerHTML =
    '<style>' +
    // Palette.ground, hairline, ink, muted, wash (Design.swift:45-50).
    ':host{all:initial}' +
    '.bar{--g:#fff;--h:#e8e8e8;--i:#171717;--m:#8c8c8c;--w:#efefef;display:flex;gap:2px;padding:3px;box-sizing:border-box;' +
    'background:var(--g);border:1px solid var(--h);border-radius:999px;opacity:.55;transition:opacity .14s ease-out;' +
    'font:12px -apple-system,sans-serif}' +
    '.bar.dark{--g:#1c1c1c;--h:#333;--i:#ededed;--m:#949494;--w:#2d2d2d}' +
    '.bar:hover{opacity:1}' +
    'button{all:unset;box-sizing:border-box;width:24px;height:22px;display:flex;align-items:center;justify-content:center;' +
    'border-radius:999px;color:var(--m);cursor:default;transition:background .14s ease-out,color .14s ease-out}' +
    'button[aria-pressed=true]{color:var(--i);background:var(--w)}' +
    '</style><div class="bar"></div>';
  var bar = shadow.querySelector('.bar');
  var buttons = {};
  var media = window.matchMedia('(prefers-color-scheme: dark)');
  function chosen() {
    return root.getAttribute('data-office-look') || 'system';
  }
  function paint() {
    var look = chosen();
    bar.classList.toggle('dark', look === 'dark' || (look === 'system' && media.matches));
    Object.keys(buttons).forEach(function (key) {
      buttons[key].setAttribute('aria-pressed', String(key === look));
    });
  }
  ['light', 'dark', 'system'].forEach(function (key) {
    var button = document.createElement('button');
    button.type = 'button';
    button.title = titles[key];
    button.setAttribute('aria-label', titles[key]);
    button.innerHTML = icons[key];
    button.addEventListener('click', function () {
      if (key === 'system') root.removeAttribute('data-office-look');
      else root.setAttribute('data-office-look', key);
      paint();
      if (typeof window.aeriReaderLook === 'function') window.aeriReaderLook(key);
    });
    buttons[key] = button;
    bar.appendChild(button);
  });
  media.addEventListener('change', paint);
  paint();
  root.appendChild(host);
})();
