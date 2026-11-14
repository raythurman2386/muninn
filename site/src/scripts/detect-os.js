// Highlight the /download/ card that matches the visitor's OS and architecture.
// Every card stays visible and usable without JavaScript.
// macOS: the current release is Apple Silicon only. Only mark "Your system"
// when we can tell the machine is arm64; never label an Intel Mac as Apple Silicon.
(function () {
  var root = document.querySelector('[data-os-cards]');
  if (!root) return;

  function apply(id) {
    if (!id) return;
    var card = root.querySelector('[data-os-card="' + id + '"]');
    if (!card) return;
    card.setAttribute('data-detected', '');
    var badge = card.querySelector('[data-detected-label]');
    if (badge) badge.hidden = false;
  }

  function haystack() {
    var ua = navigator.userAgent || '';
    var platform = '';
    try {
      platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
    } catch (e) {}
    return (platform + ' ' + ua).toLowerCase();
  }

  function macCardFromArchitecture(arch) {
    var a = String(arch || '').toLowerCase();
    if (a === 'arm' || a === 'arm64' || a === 'aarch64') return 'macos-aarch64';
    // x86 / unknown: do not claim this is the visitor's system.
    return null;
  }

  var hay = haystack();
  if (/android|iphone|ipad|ipod/.test(hay)) return;
  if (/mac/.test(hay) && !/windows/.test(hay)) {
    // "MacIntel" is reported on Apple Silicon as well, so platform alone is not enough.
    try {
      if (navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
        navigator.userAgentData.getHighEntropyValues(['architecture']).then(function (v) {
          apply(macCardFromArchitecture(v && v.architecture));
        }).catch(function () {});
      }
    } catch (e) {}
    return;
  }
  if (/win/.test(hay)) {
    apply('windows-x86_64');
    return;
  }
  if (/linux|x11|cros/.test(hay)) {
    apply(/aarch64|arm64/.test(hay) ? 'linux-aarch64' : 'linux-x86_64');
  }
})();
