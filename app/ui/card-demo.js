/* 拾字 · 极简卡片式词条库 demo
 * 用真实词库数据（183 条）渲染时间线卡片。
 * 说明：卡片上的"相对时间"是演示用假数据，词条内容 / 分组 / 包名都是真实的。
 */
(function () {
  'use strict';
  var S = window.ShiziSearch;
  var ALL = window.SHIZI_ENTRIES || [];
  var REL = ['刚刚', '12 分钟前', '48 分钟前', '2 小时前', '5 小时前', '昨天', '前天', '3 天前'];
  var PICK = 6;

  var ICON_COPY = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2.5"/><path d="M5 15V6.5A2.5 2.5 0 0 1 7.5 4H15"/></svg>';
  var ICON_STAR = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.6l2.6 5.3 5.8.85-4.2 4.1 1 5.75L12 16.9l-5.2 2.7 1-5.75-4.2-4.1 5.8-.85z"/></svg>';
  var ICON_STAR_ON = '<svg viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round"><path d="M12 3.6l2.6 5.3 5.8.85-4.2 4.1 1 5.75L12 16.9l-5.2 2.7 1-5.75-4.2-4.1 5.8-.85z"/></svg>';

  var listEl = document.getElementById('list');
  var totalEl = document.getElementById('total');
  var pinned = {};

  // 只用标题长度合适的词条，长短混排更好看
  var pool = ALL.filter(function (e) {
    var l = S.displayLabel(e).length;
    return l >= 1 && l <= 20;
  });

  function esc(s) {
    return String(s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  }

  // 词库主体是单字符符号（标题长度中位数只有 3 个字符），
  // 卡片式布局用在单符号上会显得空 —— 所以刻意混排：
  //   1 条不可见字符（验证标签兜底） + 3 条长标题术语（卡片式的主场） + 其余短符号
  var LONG = pool.filter(function (e) { return S.displayLabel(e).length >= 5; });
  var SHORT = pool.filter(function (e) { return S.displayLabel(e).length <= 3; });
  var INVIS = ALL.filter(function (e) { return S.isInvisibleOnly(e.text); });

  function sample(n) {
    var out = [];
    var taken = {};
    function take(arr, want) {
      var bag = arr.filter(function (e) { return !taken[e.id]; });
      while (want > 0 && bag.length) {
        var e = bag.splice(Math.floor(Math.random() * bag.length), 1)[0];
        taken[e.id] = 1;
        out.push(e);
        want--;
      }
    }
    take(INVIS, 1);
    take(LONG, 3);
    take(SHORT, n - out.length);
    return out;
  }

  function cardHtml(e, i) {
    var label = S.displayLabel(e);
    var isSym = e.kind === 'symbol';
    var aliases = (e.aliases || []).slice(0, 3).join(' · ');
    var metaBits = [];
    if (e.initialism) metaBits.push('<em>' + esc(e.initialism) + '</em>');
    if (aliases) metaBits.push(esc(aliases));
    return ''
      + '<article class="card" data-id="' + esc(e.id) + '">'
      +   '<div class="pills">'
      +     '<span class="pill blue">' + esc(e.packName) + '</span>'
      +     '<span class="pill">' + esc(REL[i % REL.length]) + '</span>'
      +   '</div>'
      +   '<p class="label">' + esc(e.group || (isSym ? '符号' : '术语')) + '</p>'
      +   '<h2 class="title' + (isSym ? ' sym' : '') + (label.length > 12 ? ' big' : '') + '">'
      +     esc(label) + '</h2>'
      +   (metaBits.length ? '<p class="meta">' + metaBits.join(' &nbsp;·&nbsp; ') + '</p>' : '')
      +   '<div class="bottom">'
      +     '<div class="rail"><span class="dot"></span><span class="line"></span>'
      +       '<span class="cnt">' + label.length + ' 字符</span></div>'
      +     '<div class="acts">'
      +       '<button class="circle act-star" title="收藏">' + ICON_STAR + '</button>'
      +       '<button class="circle act-copy" title="复制">' + ICON_COPY + '</button>'
      +     '</div>'
      +   '</div>'
      + '</article>';
  }

  function render() {
    var picked = sample(PICK);
    listEl.innerHTML = picked.map(cardHtml).join('');
    syncStars();
  }

  function syncStars() {
    var cards = listEl.querySelectorAll('.card');
    for (var i = 0; i < cards.length; i++) {
      var id = cards[i].getAttribute('data-id');
      var b = cards[i].querySelector('.act-star');
      var on = !!pinned[id];
      b.className = 'circle act-star' + (on ? ' on' : '');
      b.innerHTML = on ? ICON_STAR_ON : ICON_STAR;
    }
  }

  var toastEl;
  function toast(msg) {
    if (!toastEl) {
      toastEl = document.createElement('div');
      toastEl.className = 'toast';
      document.body.appendChild(toastEl);
    }
    toastEl.textContent = msg;
    toastEl.classList.add('show');
    clearTimeout(toastEl._t);
    toastEl._t = setTimeout(function () { toastEl.classList.remove('show'); }, 1400);
  }

  listEl.addEventListener('click', function (ev) {
    var card = ev.target.closest ? ev.target.closest('.card') : null;
    if (!card) return;
    var id = card.getAttribute('data-id');
    var entry = ALL.filter(function (x) { return x.id === id; })[0];

    if (ev.target.closest('.act-star')) {
      pinned[id] = !pinned[id];
      syncStars();
      toast(pinned[id] ? '已收藏' : '已取消收藏');
      return;
    }
    if (ev.target.closest('.act-copy')) {
      var text = entry ? entry.text : '';
      try {
        navigator.clipboard.writeText(text);
        toast('已复制' + (S.isInvisibleOnly(text) ? '（不可见字符，粘贴后看不到是正常的）' : ''));
      } catch (err) {
        toast('无法访问剪贴板');
      }
    }
  });

  document.getElementById('shuffle').addEventListener('click', function () {
    render();
    toast('换了一批');
  });

  totalEl.textContent = ALL.length;
  render();
})();
