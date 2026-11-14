// Highlight the /download/ card that matches the visitor's OS and architecture.
// Every card stays visible and usable without JavaScript.
(function () {
  var root = document.querySelector('[data-os-cards]');
  if (!root) return;

  function detect() {
    var ua = navigator.userAgent || '';
    var platform = '';
    try {
      platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
    } catch (e) {}
    var hay = (platform + ' ' + ua).toLowerCase();
    if (/android|iphone|ipad|ipod/.test(hay)) return null;
    if (/mac/.test(hay) && !/windows/.test(hay)) return 'macos-aarch64';
    if (/win/.test(hay)) return 'windows-x86_64';
    if (/linux|x11|cros/.test(hay)) {
      if (/aarch64|arm64/.test(hay)) return 'linux-aarch64';
      return 'linux-x86_64';
    }
    return null;
  }

  var id = detect();
  if (!id) return;
  var card = root.querySelector('[data-os-card="' + id + '"]');
  if (!card) return;
  card.setAttribute('data-detected', '');
  var badge = card.querySelector('[data-detected-label]');
  if (badge) badge.hidden = false;
})();
