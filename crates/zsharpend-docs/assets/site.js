// Zsharpend docs: copy buttons, theme, mobile menu, and search.
(function () {
  document.querySelectorAll('.copybtn').forEach(function (b) {
    b.addEventListener('click', function () {
      var code = b.parentNode.querySelector('pre code');
      if (!code || !navigator.clipboard) return;
      navigator.clipboard.writeText(code.innerText).then(function () {
        b.textContent = 'Copied';
        setTimeout(function () { b.textContent = 'Copy'; }, 1500);
      });
    });
  });

  var root = document.documentElement;
  var sel = document.getElementById('theme');
  if (sel) {
    sel.value = root.dataset.theme || '';
    sel.addEventListener('change', function () {
      if (sel.value) root.dataset.theme = sel.value; else delete root.dataset.theme;
      try { if (sel.value) localStorage.setItem('zxx-theme', sel.value); else localStorage.removeItem('zxx-theme'); } catch (e) {}
    });
  }

  var menu = document.querySelector('.mobile-bar .menu');
  if (menu) menu.addEventListener('click', function () {
    var open = document.body.classList.toggle('menu-open');
    menu.setAttribute('aria-expanded', open ? 'true' : 'false');
  });

  // ── Search (search.html?q=…) ──
  var out = document.getElementById('search-results');
  if (!out || !window.ZXX_SEARCH) return;
  var q = new URLSearchParams(location.search).get('q') || '';
  var box = document.querySelector('.search-page input[type=text]');
  if (box) box.value = q;
  var words = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return;
  function esc(s) { return s.replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); }
  function mark(s) {
    var r = esc(s);
    words.forEach(function (w) { r = r.replace(new RegExp('(' + w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'ig'), '<mark>$1</mark>'); });
    return r;
  }
  var hits = [];
  // Objects (functions, classes, modules) first, like Sphinx.
  ZXX_SEARCH.objects.forEach(function (o) {
    var n = o[0].toLowerCase();
    if (words.every(function (w) { return n.indexOf(w) >= 0 || o[1].toLowerCase().indexOf(w) >= 0; })) {
      hits.push({ score: 100 - n.length, title: o[0], url: o[2], ctx: o[1] });
    }
  });
  ZXX_SEARCH.pages.forEach(function (p) {
    var text = p.text.toLowerCase(), title = p.title.toLowerCase();
    if (!words.every(function (w) { return text.indexOf(w) >= 0 || title.indexOf(w) >= 0; })) return;
    var score = 0;
    words.forEach(function (w) { if (title.indexOf(w) >= 0) score += 20; score += Math.min(10, text.split(w).length - 1); });
    var at = text.indexOf(words[0]);
    var ctx = p.text.substr(Math.max(0, at - 60), 180);
    var url = p.url;
    p.headings.forEach(function (h) { if (h[1].toLowerCase().indexOf(words[0]) >= 0) url = p.url + '#' + h[0]; });
    hits.push({ score: score, title: p.title, url: url, ctx: (at > 60 ? '…' : '') + ctx + '…' });
  });
  hits.sort(function (a, b) { return b.score - a.score; });
  out.innerHTML = '<h2>Search results</h2>' + (hits.length
    ? '<p>Found ' + hits.length + ' result' + (hits.length === 1 ? '' : 's') + '.</p><ul>' + hits.slice(0, 50).map(function (h) {
        return '<li><a href="' + h.url + '">' + esc(h.title) + '</a><div class="ctx">' + mark(h.ctx) + '</div></li>';
      }).join('') + '</ul>'
    : '<p>Nothing matched. Check the spelling, or try fewer words.</p>');
})();
