/* 拾字 · 管理界面交互测试
 * 运行： node app/tools/test_manager.js
 *
 * 为什么需要它：静态检查（test_ui.js）看不出"视图切换没渲染"这类错。
 * 真实踩过的坑：点左栏「设置」时只调了 loadSettings()，忘了 renderSettings()，
 * 于是点了没反应 —— 右栏仍停在词条列表上。
 */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const uiDir = path.join(__dirname, '..', 'ui');
const read = (f) => fs.readFileSync(path.join(uiDir, f), 'utf8');

let fail = 0;
const chk = (ok, label, extra) => {
  if (!ok) fail++;
  console.log(`  ${ok ? 'OK  ' : 'FAIL'} ${label}${extra ? '  ' + extra : ''}`);
};

// ---------------- 最小 DOM ----------------
function makeEl(id) {
  return {
    id, innerHTML: '', textContent: '', value: '', hidden: false,
    _l: {},
    addEventListener(type, fn) { (this._l[type] = this._l[type] || []).push(fn); },
    removeEventListener() {}, focus() {}, click() {}, blur() {},
    getAttribute() { return null; }, setAttribute() {},
    closest() { return null; },
    fire(type, ev) { (this._l[type] || []).forEach((f) => f(ev || {})); }
  };
}
const els = new Map();
const el = (id) => {
  if (!els.has(id)) els.set(id, makeEl(id));
  return els.get(id);
};
const document = {
  getElementById: el,
  addEventListener() {},
  createElement: () => makeEl(''),
  body: { setAttribute() {}, getAttribute() { return null; } }
};

// ---------------- 假 Tauri ----------------
const ENTRIES = [
  { id: 'user.1', text: '测试词ABC', trigger: ';t', aliases: [], group: null,
    origin: 'user', enabled: 1, pack: null, packName: null, note: '' },
  { id: 'sym.1', text: 'μ', trigger: ';mu', aliases: ['mu'], group: null,
    origin: 'pack', enabled: 1, pack: 'symbols', packName: '符号', note: '' }
];
const SETTINGS = {
  hotkey: 'Ctrl+Alt+Space',
  hotkeyCandidates: ['Ctrl+Alt+Space', 'Ctrl+Shift+Space'],
  expandEnabled: true, autostart: false,
  dataDir: 'C:\\Users\\x\\AppData\\Roaming\\com.shizi.app',
  version: '0.1.0', firstRunDone: true
};
const invoke = (cmd) => {
  if (cmd === 'load_entries') return Promise.resolve(ENTRIES);
  if (cmd === 'get_settings') return Promise.resolve(SETTINGS);
  return Promise.resolve(null);
};

const sandbox = {
  window: { __TAURI__: { core: { invoke }, dialog: {} },
            SHIZI_THEMES: [{ id: 'minimal', name: '极简' }] },
  document,
  localStorage: { getItem() { return null; } },
  console, alert() {}, confirm() { return true; }, setTimeout, Promise
};
vm.createContext(sandbox);
vm.runInContext(read('manager.js'), sandbox, { filename: 'manager.js' });

const tick = () => new Promise((r) => setImmediate(r));

(async function main() {
  await tick(); await tick(); await tick();

  console.log('--- 打开时的列表视图 ---');
  chk(el('list').innerHTML.indexOf('测试词ABC') >= 0, '渲染出词条列表');
  chk(el('filters').innerHTML.indexOf('>设置<') >= 0, '左栏有「设置」入口');

  // 模拟用户点击左栏「设置」
  const btn = { getAttribute: (k) => (k === 'data-kind' ? 'settings' : '') };
  btn.closest = () => btn;
  el('filters').fire('click', { target: btn });
  await tick(); await tick(); await tick();

  console.log('--- 点击「设置」之后 ---');
  const h = el('list').innerHTML;
  chk(h.indexOf('全局热键') >= 0, '右栏切到设置内容（回归点：曾漏调 renderSettings）');
  chk(h.indexOf('Ctrl+Alt+Space') >= 0, '设置里显示当前热键');
  chk(el('filters').innerHTML.indexOf('data-kind="settings" data-pack="" aria-selected="true"') >= 0,
      '左栏「设置」标记为选中');
  chk(el('btnNew').hidden === true, '设置视图下「＋ 新建词条」被隐藏');

  console.log(`\n失败 ${fail} 项`);
  process.exit(fail ? 1 : 0);
})();