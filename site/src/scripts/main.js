// Muninn site: theme toggle (page chrome + product stills + hero/feature loops)
// and motion-safe video. Bundled by Astro as a module script (deferred).
// The hero and its pause button exist only on the home page; feature loops use
// [data-loop]. The theme toggle must still run when those elements are absent.
(function () {
  var root = document.documentElement;
  var KEY = 'muninn-theme';
  var reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
  var narrow = window.matchMedia('(max-width: 800px)');
  var toggle = document.querySelector('.theme-toggle');
  var hero = document.querySelector('[data-hero]');
  var heroFigure = hero && hero.closest('figure');
  var pauseBtn = heroFigure ? heroFigure.querySelector('.motion-toggle') : null;
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
      if (pauseBtn) pauseBtn.hidden = false;
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

  function loopIndex(widths) {
    return narrow.matches ? 0 : widths.length - 1;
  }

  function mountLoop(wrap) {
    var figure = wrap.closest('figure');
    var btn = figure ? figure.querySelector('.motion-toggle') : null;
    if (reduce.matches) {
      var existing = wrap.querySelector('video');
      if (existing) { existing.pause(); existing.remove(); }
      if (btn) btn.hidden = true;
      return;
    }
    var widths = wrap.getAttribute('data-loop-widths').split(',').map(Number);
    var heights = wrap.getAttribute('data-loop-heights').split(',').map(Number);
    var idx = loopIndex(widths);
    var size = widths[idx];
    var h = heights[idx];
    var scene = wrap.getAttribute('data-loop-scene');
    var base = '/assets/media/loops/' + theme() + '/loop-' + scene + '-' + size;
    var v = wrap.querySelector('video');
    if (!v) {
      v = document.createElement('video');
      v.muted = true; v.defaultMuted = true;
      v.loop = true; v.playsInline = true; v.autoplay = true;
      v.setAttribute('muted', ''); v.setAttribute('playsinline', '');
      v.setAttribute('preload', 'none');
      v.setAttribute('aria-label', wrap.getAttribute('data-loop-label') || '');
      wrap.appendChild(v);
      if (btn) {
        btn.hidden = false;
        if (!btn.getAttribute('data-loop-bound')) {
          btn.setAttribute('data-loop-bound', '1');
          btn.addEventListener('click', function () {
            var current = wrap.querySelector('video');
            if (!current) return;
            if (current.paused) {
              current.play();
              btn.textContent = 'Pause';
              btn.setAttribute('aria-pressed', 'false');
              btn.setAttribute('aria-label', 'Pause the video');
            } else {
              current.pause();
              btn.textContent = 'Play';
              btn.setAttribute('aria-pressed', 'true');
              btn.setAttribute('aria-label', 'Play the video');
            }
          });
        }
      }
    }
    if (v.getAttribute('data-loop-base') !== base) {
      v.setAttribute('data-loop-base', base);
      v.setAttribute('poster', base + '-poster.webp');
      v.setAttribute('width', String(size));
      v.setAttribute('height', String(h));
      v.removeAttribute('src');
      while (v.firstChild) v.removeChild(v.firstChild);
      var webm = document.createElement('source');
      webm.type = 'video/webm; codecs=vp9';
      webm.src = base + '.webm';
      var mp4 = document.createElement('source');
      mp4.type = 'video/mp4';
      mp4.src = base + '.mp4';
      v.appendChild(webm);
      v.appendChild(mp4);
      v.load();
    }
    if (!btn || btn.getAttribute('aria-pressed') !== 'true') {
      var playp = v.play();
      if (playp && playp.catch) playp.catch(function () {});
    }
  }

  function refreshLoops() {
    var loops = document.querySelectorAll('[data-loop]');
    for (var i = 0; i < loops.length; i++) {
      if (loops[i].getAttribute('data-loop-visible') === '1' || reduce.matches) {
        mountLoop(loops[i]);
      }
    }
  }

  if (document.querySelector('[data-loop]')) {
    var observer = new IntersectionObserver(function (entries) {
      for (var i = 0; i < entries.length; i++) {
        if (!entries[i].isIntersecting) continue;
        entries[i].target.setAttribute('data-loop-visible', '1');
        mountLoop(entries[i].target);
      }
    }, { rootMargin: '400px 0px' });
    var loops = document.querySelectorAll('[data-loop]');
    for (var i = 0; i < loops.length; i++) observer.observe(loops[i]);
    if (narrow.addEventListener) narrow.addEventListener('change', refreshLoops);
  }

  if (toggle) {
    toggle.addEventListener('click', function () {
      var next = theme() === 'dark' ? 'light' : 'dark';
      root.setAttribute('data-theme', next);
      try { localStorage.setItem(KEY, next); } catch (e) {}
      syncToggle();
      if (video) mountVideo();
      refreshLoops();
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
    syncToggle(); if (video) mountVideo(); refreshLoops();
  };
  if (scheme.addEventListener) scheme.addEventListener('change', onScheme);
  if (reduce.addEventListener) reduce.addEventListener('change', function () {
    if (hero) mountVideo();
    refreshLoops();
  });

  syncToggle();
  if (hero) mountVideo();
})();
