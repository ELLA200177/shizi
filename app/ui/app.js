/* 拾字 · 面板交互 */
(function () {
  'use strict';
  var S = window.ShiziSearch;
  var THEMES = window.SHIZI_THEMES || [];
  var entries = window.SHIZI_ENTRIES || [];
  var index = S.buildIndex(entries);
  var results = [];
  var cursor = 0;
  var host = window.__SHIZI_HOST__ || null;   // 原生宿主注入；浏览器预览时为 null

  // Tauri 宿主（build 时 withGlobalTauri=true，所以能直接拿到 window.__TAURI__）
  var tauri = window.__TAURI__;
  if (!host && tauri && tauri.core && tauri.core.invoke) {
    host = {
      commit: function (text) {
        tauri.core.invoke('commit_text', { text: text }).catch(function (e) {
          document.body.innerHTML = '<div style="padding:24px;font:13px/1.6 sans-serif;color:#667085">'
            + '注入失败：' + String(e) + '</div>';
        });
      },
      close: function () { tauri.core.invoke('hide_panel'); }
    };
  }

  var params = new URLSearchParams(location.search);
  var isEmbed = params.get('embed') === '1';
  var isPreview = !host && !isEmbed;
  document.body.classList.add(isEmbed ? 'embed' : (isPreview ? 'preview' : 'appbody'));

  // ---- 主题 ----
  function themeById(id) {
    for (var i = 0; i < THEMES.length; i++) if (THEMES[i].id === id) return THEMES[i];
    return THEMES[0];
  }
  function applyTheme(id) {
    var t = themeById(id);
    if (!t) return;
    document.body.setAttribute('data-theme', t.id);
    try { localStorage.setItem('shizi.theme', t.id); } catch (e) {}
    var nameEl = document.getElementById('swName');
    if (nameEl) nameEl.textContent = t.name;
    var dots = document.querySelectorAll('.sw-dot');
    for (var i = 0; i < dots.length; i++) {
      dots[i].setAttribute('data-on', dots[i].getAttribute('data-theme') === t.id ? '1' : '0');
    }
  }
  function initThemes() {
    var box = document.getElementById('swDots');
    if (box) {
      for (var i = 0; i < THEMES.length; i++) {
        var d = document.createElement('span');
        d.className = 'sw-dot';
        d.setAttribute('data-theme', THEMES[i].id);
        d.title = THEMES[i].name + '（' + THEMES[i].desc + '）';
        d.style.background = 'var(--dot-' + THEMES[i].id + ')';
        d.addEventListener('click', (function (id) {
          return function () { applyTheme(id); };
        })(THEMES[i].id));
        box.appendChild(d);
      }
    }
    var want = params.get('theme');
    if (!want) { try { want = localStorage.getItem('shizi.theme'); } catch (e) {} }
    applyTheme(want || THEMES[0].id);
  }

  var qEl = document.getElementById('q');
  var listEl = document.getElementById('results');
  var countEl = document.getElementById('count');
  var hintEl = document.getElementById('hint');

  function esc(s) {
    return String(s).replace(/[&<>"]/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
    });
  }

  function highlight(label) {
    var q = qEl.value.trim().toLowerCase();
    if (!q) return esc(label);
    var i = label.toLowerCase().indexOf(q);
    if (i < 0) return esc(label);
    return esc(label.slice(0, i)) + '<span class="hit">' + esc(label.slice(i, i + q.length)) + '</span>' + esc(label.slice(i + q.length));
  }

  function render() {
    results = S.search(index, qEl.value, 200).map(function (r) { return r.entry; });
    if (cursor >= results.length) cursor = Math.max(0, results.length - 1);
    if (!results.length) {
      listEl.innerHTML = '<div class="empty">没有匹配的词条</div>';
      countEl.textContent = '0 条';
      return;
    }
    var html = '';
    for (var i = 0; i < results.length; i++) {
      var e = results[i];
      html += '<div class="row" role="option" aria-selected="' + (i === cursor) + '" data-i="' + i + '">'
        + (e.trigger ? '<span class="trig">' + esc(e.trigger) + '</span>' : '')
        + '<span class="t">' + highlight(S.displayLabel(e)) + '</span>'
        + (e.group ? '<span class="pill">' + esc(e.group) + '</span>' : '')
        + '<span class="p">' + esc(e.packName || '') + '</span>'
        + '</div>';
    }
    listEl.innerHTML = html;
    countEl.textContent = results.length + ' \u6761';
    var sel = listEl.querySelector('.row[aria-selected="true"]');
    if (sel && sel.scrollIntoView) sel.scrollIntoView({ block: 'nearest' });
  }

  function move(d) {
    if (!results.length) return;
    cursor = (cursor + d + results.length) % results.length;
    var rows = listEl.querySelectorAll('.row');
    for (var i = 0; i < rows.length; i++) rows[i].setAttribute('aria-selected', String(i === cursor));
    if (rows[cursor] && rows[cursor].scrollIntoView) rows[cursor].scrollIntoView({ block: 'nearest' });
  }

  var toastEl = null;
  function toast(msg) {
    if (!toastEl) {
      toastEl = document.createElement('div');
      toastEl.className = 'toast';
      document.body.appendChild(toastEl);
    }
    toastEl.textContent = msg;
    toastEl.classList.add('show');
    clearTimeout(toastEl._t);
    toastEl._t = setTimeout(function () { toastEl.classList.remove('show'); }, 1500);
  }

  function commit() {
    var e = results[cursor];
    if (!e) return;
    if (host && host.commit) { host.commit(e.text); return; }
    try {
      navigator.clipboard.writeText(e.text);
      toast('\u9884\u89C8\u6A21\u5F0F\uFF1A\u5DF2\u590D\u5236\u5230\u526A\u8D34\u677F');
    } catch (err) {
      toast('\u9884\u89C8\u6A21\u5F0F\uFF1A\u65E0\u6CD5\u8BBF\u95EE\u526A\u8D34\u677F');
    }
  }

  function close() {
    if (host && host.close) host.close();
    else toast('\u9884\u89C8\u6A21\u5F0F\uFF1A\u6CA1\u6709\u53EF\u5173\u95ED\u7684\u7A97\u53E3');
  }

  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'ArrowDown') { ev.preventDefault(); move(1); }
    else if (ev.key === 'ArrowUp') { ev.preventDefault(); move(-1); }
    else if (ev.key === 'Enter') { ev.preventDefault(); commit(); }
    else if (ev.key === 'Escape') { ev.preventDefault(); close(); }
  });

  qEl.addEventListener('input', function () { cursor = 0; render(); });
  listEl.addEventListener('mousemove', function (ev) {
    var row = ev.target.closest ? ev.target.closest('.row') : null;
    if (!row) return;
    var i = parseInt(row.getAttribute('data-i'), 10);
    if (i !== cursor) { cursor = i; move(0); }
  });
  listEl.addEventListener('click', function (ev) {
    var row = ev.target.closest ? ev.target.closest('.row') : null;
    if (!row) return;
    cursor = parseInt(row.getAttribute('data-i'), 10);
    commit();
  });

  initThemes();
  if (params.get('q')) qEl.value = params.get('q');
  hintEl.textContent = isEmbed ? '' : (isPreview ? '\u9884\u89C8\u6A21\u5F0F\uFF08\u672A\u63A5\u5165\u6CE8\u5165\uFF09' : '\u5C31\u7EEA');

  // 显示实际注册成功的热键；一个都没注册上就明确警告（不能静默失败）
  if (host && tauri && tauri.core) {
    tauri.core.invoke('active_hotkey').then(function (k) {
      hintEl.textContent = k ? ('\u70ED\u952E ' + k) : '\u70ED\u952E\u672A\u6CE8\u518C\uFF0C\u8BF7\u7528\u6258\u76D8\u56FE\u6807\u6253\u5F00';
      if (!k) hintEl.style.color = 'var(--accent)';
    }).catch(function () {});
  }

  // 每次被热键唤起（窗口重新获得焦点）都重置查询，并重新读一次词库 ——
  // 这样在「词库管理」里改完词条，面板立刻能看到，不用重启。
  if (host) {
    window.addEventListener('focus', function () {
      qEl.value = '';
      cursor = 0;
      qEl.focus();
      render();
      if (tauri && tauri.core) {
        tauri.core.invoke('load_entries').then(function (list) {
          if (Array.isArray(list) && list.length) {
            entries = list;
            index = S.buildIndex(entries);
            render();
          }
        }).catch(function () {});
      }
    });
  }

  // 面板底部的「词库管理」入口
  var mgrLink = document.getElementById('mgrLink');
  if (mgrLink) {
    if (host && tauri && tauri.core) {
      mgrLink.hidden = false;
      mgrLink.addEventListener('click', function (ev) {
        ev.preventDefault();
        tauri.core.invoke('show_manager');
      });
    }
  }
  // 有原生宿主时，词条从 SQLite 加载 —— 用户能自己增删改；
  // 没有宿主（浏览器预览）时才用静态的 entries.js。
  if (host && tauri && tauri.core) {
    tauri.core.invoke('load_entries').then(function (list) {
      if (Array.isArray(list) && list.length) {
        entries = list;
        index = S.buildIndex(entries);
        cursor = 0;
        render();
        tauri.core.invoke('diag_log', { msg: '面板已加载 ' + entries.length + ' 条词条' });
      }
    }).catch(function (e) {
      tauri.core.invoke('diag_log', { msg: '词条加载失败: ' + String(e) });
    });
  }

  if (!isEmbed) qEl.focus();
  render();
})();
