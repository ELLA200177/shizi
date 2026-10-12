/* 拾字 · 检索核心
 * 纯函数、无 DOM 依赖。浏览器里挂到 window.ShiziSearch，node 里可 require 做测试。
 *   node app/tools/test_search.js
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.ShiziSearch = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  // 声调 -> 无声调（ü 统一写作 v，与词库约定一致）
  var TONE = {
    '\u0101':'a','\u00E1':'a','\u01CE':'a','\u00E0':'a',
    '\u014D':'o','\u00F3':'o','\u01D2':'o','\u00F2':'o',
    '\u0113':'e','\u00E9':'e','\u011B':'e','\u00E8':'e',
    '\u012B':'i','\u00ED':'i','\u01D0':'i','\u00EC':'i',
    '\u016B':'u','\u00FA':'u','\u01D4':'u','\u00F9':'u',
    '\u01D6':'v','\u01D8':'v','\u01DA':'v','\u01DC':'v','\u00FC':'v',
    '\u0144':'n','\u0148':'n','\u01F9':'n','\u1E3F':'m'
  };
  // 上下标数字/符号 -> ASCII（关键：让 "13c" 能命中 "\u03b4\u00b9\u00b3C"，不需要为每个组合写别名）
  var SCRIPT = {
    '\u2070':'0','\u00B9':'1','\u00B2':'2','\u00B3':'3','\u2074':'4',
    '\u2075':'5','\u2076':'6','\u2077':'7','\u2078':'8','\u2079':'9',
    '\u2080':'0','\u2081':'1','\u2082':'2','\u2083':'3','\u2084':'4',
    '\u2085':'5','\u2086':'6','\u2087':'7','\u2088':'8','\u2089':'9',
    '\u207A':'+','\u207B':'-','\u207F':'n'
  };
  var SEP = /[\s\-_.\u00B7\u3001\uFF0C\u3002;\uFF1B:\uFF1A!\uFF01?\uFF1F\/\\|()\uFF08\uFF09\[\]\u3010\u3011{}<>\u300A\u300B"'\u201C\u201D\u2018\u2019]/g;
  var TONE_RE = /[\u0101\u00E1\u01CE\u00E0\u014D\u00F3\u01D2\u00F2\u0113\u00E9\u011B\u00E8\u012B\u00ED\u01D0\u00EC\u016B\u00FA\u01D4\u00F9\u01D6\u01D8\u01DA\u01DC\u00FC\u0144\u0148\u01F9\u1E3F]/g;
  var SCRIPT_RE = /[\u2070\u00B9\u00B2\u00B3\u2074-\u2079\u2080-\u2089\u207A\u207B\u207F]/g;

  function canonical(input) {
    if (input === null || input === undefined) return '';
    var t = String(input).toLowerCase();
    t = t.replace(/[\uFF01-\uFF5E]/g, function (c) {
      return String.fromCharCode(c.charCodeAt(0) - 0xFEE0);
    });
    t = t.replace(/\u3000/g, ' ');
    t = t.replace(TONE_RE, function (c) { return TONE[c] || c; });
    t = t.replace(SCRIPT_RE, function (c) { return SCRIPT[c] || c; });
    t = t.replace(/\u00B5/g, '\u03BC');   // 微符号归一为希腊 mu：检索时同一键，插入时保留原字符
    t = t.replace(SEP, '');
    return t;
  }

  function buildIndex(entries) {
    return (entries || []).map(function (e) {
      var ct = canonical(e.text);
      var cal = (e.aliases || []).map(canonical).filter(Boolean);
      var cpy = canonical(e.pinyin || '');
      var cin = canonical(e.initialism || '');
      var out = {};
      for (var k in e) if (Object.prototype.hasOwnProperty.call(e, k)) out[k] = e[k];
      out._ct = ct;
      out._calArr = cal;
      out._cpy = cpy;
      out._cin = cin;
      // 触发词也进检索串：输入 ;gdzxbx 或 gdzxbx 都能命中
      out._blob = [ct].concat(cal, [cpy, cin, canonical(e.trigger || ''), canonical(e.note || ''), canonical(e.group || '')]).join('|');
      return out;
    });
  }

  function matchWeight(e, q) {
    if (!q) return 0;
    if (e._ct === q) return 1.00;
    if (e._calArr.indexOf(q) >= 0) return 0.95;
    if (e._cin && e._cin === q) return 0.90;
    if (e._cpy && e._cpy.indexOf(q) === 0) return 0.80;
    if (e._ct.indexOf(q) === 0) return 0.80;
    for (var i = 0; i < e._calArr.length; i++) {
      if (e._calArr[i].indexOf(q) === 0) return 0.80;
    }
    if (e._blob.indexOf(q) >= 0) return 0.60;
    return 0;
  }

  function search(index, query, limit) {
    var q = canonical(query);
    var out = [];
    if (!q) {
      // 空查询也必须返回与普通查询**同构**的结果（{entry, weight, score}）。
      // 曾经这里直接返回词条对象，而调用方统一按 r.entry 取值 ——
      // 结果面板一打开（搜索框为空）就抛异常、列表全空。
      out = index.slice();
      out.sort(function (a, b) {
        return (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0) || (b.useCount || 0) - (a.useCount || 0);
      });
      if (limit) out = out.slice(0, limit);
      return out.map(function (e) { return { entry: e, weight: 0, score: 0 }; });
    }
    for (var i = 0; i < index.length; i++) {
      var e = index[i];
      var w = matchWeight(e, q);
      if (w <= 0) continue;
      var use = Math.min(1, Math.log10(1 + (e.useCount || 0)) / 3);
      out.push({ entry: e, weight: w, score: 1000 * w + 200 * use + 300 * (e.pinned ? 1 : 0) - 0.5 * e._ct.length });
    }
    out.sort(function (a, b) {
      return b.score - a.score || a.entry._ct.length - b.entry._ct.length;
    });
    return limit ? out.slice(0, limit) : out;
  }

  // 不可见字符：标签里不能直接显示，否则用户以为条目是空的
  var INVISIBLE = {
    '\u00A0': 'NBSP U+00A0',
    '\u200B': 'ZWSP U+200B',
    '\u3000': '\u5168\u89D2\u7A7A\u683C U+3000'
  };
  function isInvisibleOnly(t) {
    if (!t || t.trim() === '') return true;
    for (var i = 0; i < t.length; i++) {
      if (!INVISIBLE[t[i]]) return false;
    }
    return true;
  }
  function displayLabel(e) {
    var t = e.text === undefined ? '' : e.text;
    if (!isInvisibleOnly(t)) return t;
    var parts = [];
    for (var i = 0; i < t.length; i++) {
      var c = t[i];
      parts.push(INVISIBLE[c] || ('U+' + c.charCodeAt(0).toString(16).toUpperCase().padStart(4, '0')));
    }
    return '\u27E8' + parts.join(' ') + '\u27E9';
  }

  return {
    canonical: canonical,
    buildIndex: buildIndex,
    matchWeight: matchWeight,
    search: search,
    isInvisibleOnly: isInvisibleOnly,
    displayLabel: displayLabel
  };
});
