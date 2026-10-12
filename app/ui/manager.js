/* 拾字 · 词库管理
 *
 * 设计上的两个刻意选择：
 *  1. **不让用户"想"触发码**。输入内容后自动给出建议，点一下就用 ——
 *     「必须记触发码」是 Espanso 的缺点，不该装回自己的产品。
 *  2. **官方包只能停用、不能改**。否则包升级时用户的修改会被冲掉，
 *     或者反过来永久盖住新版。
 */
(function () {
  'use strict';
  var tauri = window.__TAURI__;
  var core = tauri && tauri.core ? tauri.core : null;
  var THEMES = window.SHIZI_THEMES || [];

  var entries = [];
  var view = 'list';        // list | settings
  var settings = null;
  var filterKind = 'all';   // all | user | pack
  var filterPack = null;
  var query = '';
  var editing = null;       // 正在编辑的 id；null = 新建

  // ---- 主题跟随面板的选择 ----
  (function applyTheme() {
    var t = null;
    try { t = localStorage.getItem('shizi.theme'); } catch (e) {}
    var ok = THEMES.some(function (x) { return x.id === t; });
    document.body.setAttribute('data-theme', ok ? t : ((THEMES[0] || {}).id || 'minimal'));
  })();

  function el(id) { return document.getElementById(id); }
  function val(id) { return (el(id).value || '').trim(); }
  function esc(s) {
    return String(s === undefined || s === null ? '' : s)
      .replace(/[&<>"]/g, function (c) {
        return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c];
      });
  }
  function invoke(cmd, args) { return core.invoke(cmd, args); }
  // 凡是用户点出来的动作都要有失败提示 —— 之前 open_data_dir 没 catch，
  // 出错时界面毫无反应，只能靠猜。
  function run(cmd, args, label) {
    return invoke(cmd, args).catch(function (e) {
      window.alert((label || cmd) + ' 失败：' + e);
      throw e;
    });
  }
  function diag(msg) { if (core) invoke('diag_log', { msg: '管理界面: ' + msg }).catch(function () {}); }
  function isUser(e) { return e.origin === 'user'; }
  function on(e) { return e.enabled !== 0; }

  // ---------------- 数据 ----------------

  function load() {
    if (!core) {
      el('list').innerHTML = '<div class="mgr-empty">这个界面需要在拾字应用里打开</div>';
      return Promise.resolve();
    }
    // 管理界面必须看到被停用的词条，否则停用之后就再也启用不回来
    return invoke('load_entries', { includeDisabled: true }).then(function (list) {
      entries = Array.isArray(list) ? list : [];
      renderFilters();
      renderList();
      loadSettings();
      diag('加载 ' + entries.length + ' 条（含停用）');
    }).catch(function (e) {
      el('list').innerHTML = '<div class="mgr-empty">加载失败：' + esc(e) + '</div>';
      diag('加载失败 ' + e);
    });
  }

  function filtered() {
    var q = query.toLowerCase();
    return entries.filter(function (e) {
      if (filterKind === 'user' && !isUser(e)) return false;
      if (filterKind === 'pack' && isUser(e)) return false;
      if (filterPack && e.pack !== filterPack) return false;
      if (!q) return true;
      var hay = [e.text, e.trigger, e.group, e.note, (e.aliases || []).join(' '), e.packName]
        .join(' ').toLowerCase();
      return hay.indexOf(q) >= 0;
    }).sort(function (a, b) {
      if (isUser(a) !== isUser(b)) return isUser(a) ? -1 : 1;   // 我的词条排前面
      return a.id < b.id ? -1 : 1;
    });
  }

  // ---------------- 渲染 ----------------

  function renderCounts() {
    var user = entries.filter(isUser).length;
    var packs = entries.filter(function (e) { return !isUser(e); }).length;
    el('stat').textContent = entries.length + ' 条 · 我的 ' + user;
    return { user: user, packs: packs };
  }

  function renderFilters() {
    var c = renderCounts();
    var byPack = {};
    entries.filter(function (e) { return !isUser(e); }).forEach(function (e) {
      var k = e.pack || 'unknown';
      byPack[k] = byPack[k] || { name: e.packName || k, n: 0 };
      byPack[k].n++;
    });

    function item(kind, label, n, sub, pack) {
      var sel = (filterKind === kind && filterPack === (pack || null));
      return '<button data-kind="' + kind + '" data-pack="' + esc(pack || '') + '"' +
        (sel ? ' aria-selected="true"' : '') + ' class="' + (sub ? 'sub' : '') + '">' +
        '<span>' + esc(label) + '</span><span class="n">' + n + '</span></button>';
    }

    var html = item('all', '全部词条', entries.length, false, null);
    html += item('user', '我的词条', c.user, false, null);
    html += item('pack', '官方词库包', c.packs, false, null);
    Object.keys(byPack).sort().forEach(function (k) {
      html += item('pack', byPack[k].name, byPack[k].n, true, k);
    });
    html += '<button data-kind="settings" data-pack=""' +
      (view === 'settings' ? ' aria-selected="true"' : '') +
      ' style="margin-top:10px;border-top:1px solid var(--line);border-radius:0;padding-top:12px">' +
      '<span>设置</span></button>';
    el('filters').innerHTML = html;
  }

  function welcomeHtml() {
    if (!settings || settings.firstRunDone) return '';
    var hk = settings.hotkey || 'Ctrl+Alt+Space';
    return '<div class="welcome"><h3>欢迎使用拾字</h3><ol>' +
      '<li>按 <code>' + esc(hk) + '</code> 唤出取词面板，打字搜索，回车插入</li>' +
      '<li>也可以<b>直接敲触发码</b>：<code>;mu</code> 出 μ，<code>;13c</code> 出 δ¹³C</li>' +
      '<li>要加自己的词条，点左下角「＋ 新建词条」—— 输入内容后会自动给你触发码建议</li>' +
      '</ol><div class="w-act"><button class="mgr-btn primary" id="btnWelcomeOk">知道了</button></div></div>';
  }

  function renderList() {
    if (view === 'settings') { renderSettings(); return; }
    var list = filtered();
    if (!list.length) {
      var user = entries.filter(isUser).length;
      el('list').innerHTML = welcomeHtml() + '<div class="mgr-empty">' +
        (entries.length === 0 ? '词库是空的'
          : (filterKind === 'user' && user === 0 ? '还没有你自己的词条 —— 点左下角「＋ 新建词条」试试'
            : '没有匹配的词条')) + '</div>';
      return;
    }
    el('list').innerHTML = welcomeHtml() + list.map(function (e) {
      var acts = '';
      if (isUser(e)) {
        acts += '<button class="mgr-mini" data-act="edit" data-id="' + esc(e.id) + '">编辑</button>';
      }
      acts += '<button class="mgr-mini" data-act="toggle" data-id="' + esc(e.id) + '">' +
        (on(e) ? '停用' : '启用') + '</button>';
      return '<div class="mgr-row' + (on(e) ? '' : ' off') + '">' +
        '<span class="trig">' + esc(e.trigger || '—') + '</span>' +
        '<span class="txt">' + esc(e.text) + '</span>' +
        (e.group ? '<span class="grp">' + esc(e.group) + '</span>' : '') +
        '<span class="src">' + (isUser(e) ? '我的' : '官方') + '</span>' +
        '<span class="acts">' + acts + '</span>' +
        '</div>';
    }).join('');
  }

  function toggleRow(label, key, on, hint) {
    return '<div class="set-row"><div><div class="set-label">' + esc(label) + '</div>' +
      (hint ? '<div class="set-hint">' + esc(hint) + '</div>' : '') + '</div>' +
      '<button class="mgr-mini" data-set="' + key + '">' + (on ? '已开启' : '已关闭') + '</button></div>';
  }

  function renderSettings() {
    el('btnNew').hidden = true;
    el('btnExportPack').hidden = true;
    if (!settings) { el('list').innerHTML = '<div class="mgr-empty">读取设置中…</div>'; return; }
    var s = settings;
    var hks = (s.hotkeyCandidates || []).map(function (h) {
      return '<button class="hk' + (h === s.hotkey ? ' on' : '') + '" data-hk="' + esc(h) + '">' +
        esc(h) + '</button>';
    }).join('');
    el('list').innerHTML =
      '<div class="set-block"><div class="set-title">全局热键</div>' +
        '<div class="set-hint">当前生效：<b>' + esc(s.hotkey || '未注册') + '</b>。' +
        '点其它候选立刻切换 —— 若这个键被别的程序占用，会告诉你并退回可用键。</div>' +
        '<div class="hk-row">' + hks + '</div></div>' +
      '<div class="set-block"><div class="set-title">行为</div>' +
        toggleRow('打字自动展开', 'expand_enabled', s.expandEnabled,
          '在任意程序里敲触发码直接出词。关闭会摘掉键盘钩子，不再接收任何按键。') +
        toggleRow('开机自启', 'autostart', s.autostart, '登录后在托盘自动运行') +
      '</div>' +
      '<div class="set-block"><div class="set-title">词库包</div>' +
        '<div class="set-hint">导出的文件可以发给别人；对方用「导入词库包」加载即可。' +
        '导入时如果触发码撞车，词条会照常进来，只是降级为只能搜索。</div>' +
        '<div class="set-row"><button class="mgr-btn" id="btnExportMine">导出我的词条</button>' +
        '<button class="mgr-btn" id="btnImport">导入词库包…</button></div>' +
      '</div>' +
      '<div class="set-block"><div class="set-title">数据</div>' +
        '<div class="set-hint mono">' + esc(s.dataDir) + '</div>' +
        '<div class="set-hint">词库就是这个文件 —— 想备份，直接把它复制走即可。' +
        '「在资源管理器中打开」会进入该文件夹。</div>' +
        '<div class="set-row"><button class="mgr-btn" id="btnOpenDir">在资源管理器中打开</button>' +
        '<button class="mgr-btn" id="btnCopyPath">复制路径</button></div>' +
      '</div>' +
      '<div class="set-block"><div class="set-title">隐私</div>' +
        '<div class="set-hint">拾字会监听键盘（仅用于识别触发码）并读写剪贴板（仅用于插入文本）。' +
        '不联网、不上传任何内容。关闭「打字自动展开」会直接摘掉键盘钩子。</div>' +
      '</div>' +
      '<div class="set-block"><div class="set-title">关于</div>' +
        '<div class="set-hint">拾字 v' + esc(s.version) + '</div></div>';
  }

  el('list').addEventListener('click', function (ev) {
    var t = ev.target;
    var g = function (sel) { return t.closest ? t.closest(sel) : null; };

    if (t.id === 'btnWelcomeOk') {
      invoke('set_setting', { key: 'first_run_done', value: '1' }).then(function () {
        return loadSettings().then(renderList);
      });
      return;
    }
    var hk = g('button[data-hk]');
    if (hk) {
      invoke('set_hotkey', { label: hk.getAttribute('data-hk') }).then(function () {
        return loadSettings().then(renderSettings);
      }).catch(function (e) {
        window.alert(String(e));
        loadSettings().then(renderSettings);
      });
      return;
    }
    var stb = g('button[data-set]');
    if (stb) {
      var key = stb.getAttribute('data-set');
      var cur = key === 'expand_enabled' ? settings.expandEnabled : settings.autostart;
      invoke('set_setting', { key: key, value: cur ? '0' : '1' }).then(function () {
        return loadSettings().then(renderSettings);
      }).catch(function (e) { window.alert(String(e)); });
      return;
    }
    if (t.id === 'btnOpenDir') { run('open_data_dir', undefined, '定位数据目录'); return; }
    if (t.id === 'btnCopyPath') {
      run('copy_to_clipboard', { text: settings.dataDir }, '复制路径')
        .then(function () { window.alert('路径已复制：\n' + settings.dataDir + '\n\n粘进资源管理器地址栏即可打开'); })
        .catch(function () {});
      return;
    }
    if (t.id === 'btnExportMine') { doExport(null); return; }
    if (t.id === 'btnExportPack') { doExport(filterPack); return; }
    if (t.id === 'btnImport') { doImport(); return; }

    var b = g('button[data-act]');
    if (!b) return;
    var id = b.getAttribute('data-id');
    var e = entries.find(function (x) { return x.id === id; });
    if (!e) return;
    if (b.getAttribute('data-act') === 'edit') { openForm(e); return; }
    invoke('toggle_entry', { id: id, on: !on(e) })
      .then(load)
      .catch(function (err) { diag('切换失败 ' + err); });
  });

  el('filters').addEventListener('click', function (ev) {
    var b = ev.target.closest ? ev.target.closest('button[data-kind]') : null;
    if (!b) return;
    var kind = b.getAttribute('data-kind');
    view = kind === 'settings' ? 'settings' : 'list';
    if (view === 'list') {
      filterKind = kind;
      filterPack = b.getAttribute('data-pack') || null;
    }
    el('btnNew').hidden = view === 'settings';
    el('btnExportPack').hidden = !(view === 'list' && filterPack);
    renderFilters();
    if (view === 'settings') loadSettings().then(renderSettings); else renderList();
  });

  el('search').addEventListener('input', function () {
    query = el('search').value.trim();
    renderList();
  });

  // ---------------- 表单 ----------------

  function openForm(entry) {
    editing = entry ? entry.id : null;
    el('formTitle').textContent = entry ? '编辑词条' : '新建词条';
    el('fText').value = entry ? entry.text : '';
    el('fTrigger').value = entry ? (entry.trigger || '') : '';
    el('fAliases').value = entry ? (entry.aliases || []).join(', ') : '';
    el('fGroup').value = entry ? (entry.group || '') : '';
    el('btnDelete').hidden = !entry;
    hideError();
    el('modal').hidden = false;
    updateNote();
    if (!entry) updateSuggest();
    setTimeout(function () { el('fText').focus(); }, 30);
  }

  function closeForm() {
    el('modal').hidden = true;
    editing = null;
  }

  function showError(msg) { el('formError').textContent = msg; el('formError').hidden = false; }
  function hideError() { el('formError').hidden = true; }

  var sugTimer = null;
  function updateSuggest() {
    clearTimeout(sugTimer);
    var text = val('fText');
    if (!text || !core) { el('suggest').innerHTML = ''; return; }
    sugTimer = setTimeout(function () {
      invoke('suggest_triggers', { text: text }).then(function (list) {
        var arr = Array.isArray(list) ? list : [];
        if (!arr.length) { el('suggest').innerHTML = ''; return; }
        el('suggest').innerHTML = '<span style="font-size:11.5px;color:var(--fg-faint);align-self:center">建议</span>' +
          arr.map(function (s) {
            var tv = ';' + s;
            var taken = entries.some(function (e) {
              return e.trigger === tv && e.id !== editing && on(e);
            });
            return '<button data-sug="' + esc(tv) + '"' + (taken ? ' class="taken" disabled' : '') + '>' +
              esc(tv) + '</button>';
          }).join('');
      }).catch(function () {});
    }, 250);
  }

  el('suggest').addEventListener('click', function (ev) {
    var b = ev.target.closest ? ev.target.closest('button[data-sug]') : null;
    if (!b || b.disabled) return;
    el('fTrigger').value = b.getAttribute('data-sug');
    updateNote();
  });

  // 触发码冲突检测：报错要说清被谁占了，别让用户猜
  function updateNote() {
    var note = el('note');
    var raw = val('fTrigger');
    if (!raw) { note.className = 'mgr-note'; note.textContent = '留空则只能用面板搜索'; return; }
    var tv = raw.charAt(0) === ';' ? raw : ';' + raw;

    var owner = entries.find(function (e) {
      return e.trigger === tv && e.id !== editing && on(e);
    });
    if (owner) {
      note.className = 'mgr-note err';
      note.textContent = '✕ ' + tv + ' 已被占用：' + owner.text.slice(0, 24);
      return;
    }
    var pref = entries.find(function (e) {
      if (e.id === editing || !e.trigger || !on(e)) return false;
      return e.trigger !== tv && (e.trigger.indexOf(tv) === 0 || tv.indexOf(e.trigger) === 0);
    });
    if (pref) {
      note.className = 'mgr-note warn';
      note.textContent = '⚠ 与 ' + pref.trigger + ' 存在前缀关系 —— 更长的那个会优先命中';
      return;
    }
    note.className = 'mgr-note';
    note.textContent = '✓ ' + tv + ' 可用';
  }

  el('fTrigger').addEventListener('input', updateNote);
  el('fText').addEventListener('input', function () { updateSuggest(); });

  el('btnSave').addEventListener('click', function () {
    var text = val('fText');
    if (!text) { showError('「要插入的内容」不能为空'); return; }
    hideError();
    var payload = {
      id: editing,
      text: text,
      trigger: val('fTrigger') || null,
      group: val('fGroup') || null,
      note: null,
      aliases: val('fAliases').split(/[,，]/).map(function (s) { return s.trim(); }).filter(Boolean)
    };
    invoke('save_entry', { input: payload }).then(function () {
      closeForm();
      return load();
    }).catch(function (e) { showError(String(e)); });
  });

  el('btnDelete').addEventListener('click', function () {
    if (!editing) return;
    var e = entries.find(function (x) { return x.id === editing; });
    if (!window.confirm('删除「' + (e ? e.text.slice(0, 20) : '') + '」？此操作不可撤销。')) return;
    invoke('delete_entry', { id: editing }).then(function () {
      closeForm();
      return load();
    }).catch(function (err) { showError(String(err)); });
  });

  el('btnNew').addEventListener('click', function () { openForm(null); });
  el('btnCancel').addEventListener('click', closeForm);
  el('modal').addEventListener('click', function (ev) { if (ev.target === el('modal')) closeForm(); });
  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'Escape' && !el('modal').hidden) closeForm();
    if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter' && !el('modal').hidden) el('btnSave').click();
  });

  function loadSettings() {
    if (!core) return Promise.resolve();
    return invoke('get_settings').then(function (s) { settings = s; })
      .catch(function (e) { diag('读设置失败 ' + e); });
  }

  function doExport(packId) {
    var dlg = window.__TAURI__ && window.__TAURI__.dialog;
    if (!dlg) { window.alert('这个界面需要在拾字应用里打开'); return; }
    dlg.save({
      defaultPath: (packId || 'mine') + '.json',
      filters: [{ name: '拾字词库包', extensions: ['json'] }]
    }).then(function (p) {
      if (!p) return;
      return invoke('export_pack', { packId: packId || null, path: p })
        .then(function () { window.alert('已导出到：\n' + p); })
        .catch(function (e) { window.alert('导出失败：' + e); });
    });
  }

  function doImport() {
    var dlg = window.__TAURI__ && window.__TAURI__.dialog;
    if (!dlg) { window.alert('这个界面需要在拾字应用里打开'); return; }
    dlg.open({ multiple: false, filters: [{ name: '拾字词库包', extensions: ['json'] }] })
      .then(function (p) {
        if (!p) return;
        var path = Array.isArray(p) ? p[0] : p;
        return invoke('preview_import', { path: path }).then(function (pv) {
          var msg = '词库包：' + pv.packName + '（' + pv.packId + '）\n词条：' + pv.total + ' 条\n';
          if (pv.replacing) msg += '已存在同 id 的包，这次是更新它\n';
          if (pv.triggerConflicts) {
            msg += '\n有 ' + pv.triggerConflicts + ' 条的触发码已被占用，' +
              '这些词条会照常导入，但只能用面板搜索：\n  ' + pv.conflicts.join('\n  ') + '\n';
          }
          if (!window.confirm(msg + '\n确定导入吗？')) return;
          return invoke('apply_import', { path: path }).then(function (r) {
            window.alert('已导入 ' + r.imported + ' 条' +
              (r.triggerDropped ? '，其中 ' + r.triggerDropped + ' 条因触发码冲突降级为仅搜索' : ''));
            return load();
          });
        });
      })
      .catch(function (e) { window.alert('导入失败：' + e); });
  }

  el('hint').textContent = '左栏底部有「设置」：换热键、开关打字展开、导入导出词库包';
  load();
})();
