// Muninn site: theme toggle (page chrome + product stills + hero loop) and a motion-safe hero video.
// Bundled by Astro as a module script (deferred, like the old <script defer src="main.js">).
// The hero and pause button exist only on the home page; the theme toggle must still
// run on /privacy/ and the 404 page when those elements are absent.
(function () {
  var root = document.documentElement;
  var KEY = 'muninn-theme';
  var reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
  var toggle = document.querySelector('.theme-toggle');
  var hero = document.querySelector('[data-hero]');
  var pauseBtn = document.querySelector('.motion-toggle');
  var video = null;
  var userPaused = false;

  function theme() { return root.getAttribute('data-theme') === 'light' ? 'light' : 'dark'; }

  function syncToggle() {
    if (!toggle) return;
    var next = theme() === 'dark' ? 'light' : 'dark';
    toggle.setAttribute('aria-label', 'Use ' + next + ' theme');
    toggle.setAttribute('aria-pressed', String(theme() === 'light'));
    toggle.querySelector('.toggle-label').textContent = next === 'light' ? 'Light' : 'Dark';
    var meta = document.querySelectorAll('meta[name="theme-color"]');
    for (var i = 0; i < meta.length; i++) meta[i].setAttribute('content', theme() === 'dark' ? '#080808' : '#fafafa');
  }

  function heroBase() { return '/assets/video/' + theme() + '-hero-tour'; }
  function posterSrc() { return '/assets/img/' + theme() + '/preview-1600.webp'; }

  function applyHeroSources() {
    var base = heroBase();
    if (video.getAttribute('data-hero-base') === base) return;
    video.setAttribute('data-hero-base', base);
    video.setAttribute('poster', posterSrc());
    video.removeAttribute('src');
    while (video.firstChild) video.removeChild(video.firstChild);
    var webm = document.createElement('source');
    webm.type = 'video/webm; codecs=vp9';
    webm.src = base + '.webm';
    var mp4 = document.createElement('source');
    mp4.type = 'video/mp4';
    mp4.src = base + '.mp4';
    video.appendChild(webm);
    video.appendChild(mp4);
    video.load();
  }

  function mountVideo() {
    if (!hero || !pauseBtn) return;
    if (reduce.matches) { unmountVideo(); return; }
    if (!video) {
      video = document.createElement('video');
      video.muted = true; video.defaultMuted = true;
      video.loop = true; video.playsInline = true; video.autoplay = true;
      video.setAttribute('muted', ''); video.setAttribute('playsinline', '');
      video.setAttribute('preload', 'auto');
      video.setAttribute('aria-label', 'Muninn tour: Read a note, search, glide the graph, then open Board and Kanban.');
      hero.appendChild(video);
      pauseBtn.hidden = false;
    }
    applyHeroSources();
    if (!userPaused) { var p = video.play(); if (p && p.catch) p.catch(function () {}); }
    syncPause();
  }

  function unmountVideo() {
    if (video) { video.pause(); video.remove(); video = null; }
    if (pauseBtn) pauseBtn.hidden = true;
  }

  function syncPause() {
    if (!video || !pauseBtn) return;
    var paused = userPaused;
    pauseBtn.textContent = paused ? 'Play' : 'Pause';
    pauseBtn.setAttribute('aria-pressed', String(paused));
    pauseBtn.setAttribute('aria-label', paused ? 'Play the tour video' : 'Pause the tour video');
  }

  if (toggle) {
    toggle.addEventListener('click', function () {
      var next = theme() === 'dark' ? 'light' : 'dark';
      root.setAttribute('data-theme', next);
      try { localStorage.setItem(KEY, next); } catch (e) {}
      syncToggle();
      if (video) mountVideo();
    });
  }

  if (pauseBtn) {
    pauseBtn.addEventListener('click', function () {
      if (!video) return;
      userPaused = !userPaused;
      if (userPaused) video.pause(); else video.play();
      syncPause();
    });
  }

  // Follow the system appearance live, unless the visitor picked a theme here.
  var scheme = window.matchMedia('(prefers-color-scheme: light)');
  var onScheme = function () {
    var saved = null; try { saved = localStorage.getItem(KEY); } catch (e) {}
    if (saved) return;
    root.setAttribute('data-theme', scheme.matches ? 'light' : 'dark');
    syncToggle(); if (video) mountVideo();
  };
  if (scheme.addEventListener) scheme.addEventListener('change', onScheme);
  if (reduce.addEventListener) reduce.addEventListener('change', mountVideo);

  syncToggle();
  mountVideo();
})();
