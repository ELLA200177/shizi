/* 拾字 · UI 一致性测试
 * 运行： node app/tools/test_ui.js
 * 本测试的价值：CSS 的视觉效果无法自动验证，但"引用了不存在的变量"这类
 * 错误可以直接抓出来 —— 之前 --dot-* 漏定义就是这种错。
 */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const uiDir = path.join(__dirname, '..', 'ui');
const read = (f) => fs.readFileSync(path.join(uiDir, f), 'utf8');
const css = read('styles.css');
const appJs = read('app.js');
const galleryJs = read('gallery.js');
const indexHtml = read('index.html');

let fail = 0;
const chk = (ok, label, extra) => {
  if (!ok) fail++;
  console.log(`  ${ok ? 'OK  ' : 'FAIL'} ${label}${extra ? '  ' + extra : ''}`);
};

// --- 加载 themes.js ---
const sandbox = { window: {} };
vm.createContext(sandbox);
vm.runInContext(read('themes.js'), sandbox, { filename: 'themes.js' });
const THEMES = sandbox.window.SHIZI_THEMES;
console.log('--- 主题清单 ---');
chk(Array.isArray(THEMES) && THEMES.length > 0, 'themes.js 可解析', `${THEMES.length} 个主题`);
chk(new Set(THEMES.map(t => t.id)).size === THEMES.length, '主题 id 不重复');

// --- 每个主题都要有 CSS 块 ---
console.log('--- 主题 / CSS 一一对应 ---');
for (const t of THEMES) {
  const has = css.indexOf(`body[data-theme="${t.id}"]`) >= 0;
  chk(has, `body[data-theme="${t.id}"] 存在`, t.name);
  const dot = css.indexOf(`--dot-${t.id}:`) >= 0;
  chk(dot, `--dot-${t.id} 已定义`);
}
const cssThemeIds = [...css.matchAll(/body\[data-theme="([a-z0-9-]+)"\]/g)].map(m => m[1]);
const extra = cssThemeIds.filter(id => !THEMES.some(t => t.id === id));
chk(extra.length === 0, 'CSS 里没有多余主题块', extra.length ? JSON.stringify(extra) : '');

// --- 所有 var(--x) 引用都必须有定义 ---
console.log('--- CSS 变量引用完整性 ---');
const defined = new Set([...css.matchAll(/(^|[\s;{])(--[a-zA-Z0-9-]+)\s*:/g)].map(m => m[2]));
const used = new Set();
for (const src of [css, appJs, galleryJs, indexHtml]) {
  for (const m of src.matchAll(/var\((--[a-zA-Z0-9-]+)/g)) used.add(m[1]);
}
// 以连字符结尾的是"运行时拼接前缀"（如 'var(--dot-' + id + ')'），
// 单独校验其展开结果，不按普通变量处理
const dynamicPrefixes = [...used].filter(v => v.endsWith('-'));
const staticUsed = [...used].filter(v => !v.endsWith('-'));
const missing = staticUsed.filter(v => !defined.has(v));
chk(missing.length === 0, `${staticUsed.length} 个静态变量引用全部有定义`,
    missing.length ? '缺失: ' + JSON.stringify(missing) : '');
for (const pre of dynamicPrefixes) {
  const bad = THEMES.filter(t => css.indexOf(`${pre}${t.id}:`) < 0).map(t => t.id);
  chk(bad.length === 0, `拼接前缀 ${pre} 对每个主题都有定义`, bad.length ? '缺失: ' + JSON.stringify(bad) : '');
}

// 每个主题必须覆盖这几项，否则是写了一半的主题
const REQUIRED = ['--bg', '--bg-glass', '--fg', '--fg-dim', '--fg-faint', '--accent',
  '--accent-soft', '--line', '--sel-bg', '--sel-bar', '--pill-bg', '--kbd-bg',
  '--backdrop-base', '--shadow'];
for (const t of THEMES) {
  const m = css.match(new RegExp('body\\[data-theme="' + t.id + '\\"]\\{([\\s\\S]*?)\\}'));
  const body = m ? m[1] : '';
  const lack = REQUIRED.filter(v => body.indexOf(v + ':') < 0);
  chk(lack.length === 0, `主题 ${t.id} 覆盖了全部必需变量`, lack.length ? '缺: ' + JSON.stringify(lack) : '');
}

// --- 内联 var(--dot-X) 拼接必须与主题 id 对齐 ---
console.log('--- 切换器圆点拼接 ---');
chk(appJs.indexOf("'var(--dot-' + THEMES[i].id + ')'") >= 0, 'app.js 用 --dot-<id> 拼接圆点颜色');

// --- app.js 渲染出的 class 必须在 styles.css 里有定义（防止改名后留死样式/幽灵类名）---
console.log('--- 渲染类名 / 样式对账 ---');
const emitted = new Set();
for (const m of appJs.matchAll(/class(?:Name)?\s*=\s*['"]([^'"]+)['"]/g)) {
  m[1].split(/\s+/).filter(Boolean).forEach(c => emitted.add(c));
}
const undefinedCls = [...emitted].filter(c => css.indexOf('.' + c) < 0);
chk(undefinedCls.length === 0, `${emitted.size} 个渲染类名都有样式`,
    undefinedCls.length ? '无样式: ' + JSON.stringify(undefinedCls) : JSON.stringify([...emitted]));
// 反向：旧样式里遗留的、已无人使用的类名（.badge / .m 是上一版设计留下的）
const stale = ['badge', 'm'].filter(c => css.indexOf('.' + c + '{') >= 0 || css.indexOf('.' + c + ' ') >= 0 || css.indexOf('.' + c + ',') >= 0);
chk(stale.length === 0, '没有上一版遗留的死样式', stale.length ? '遗留: ' + JSON.stringify(stale) : '');

// --- 关键文件都被 index.html 引入 ---
console.log('--- index.html 引用完整性 ---');
for (const f of ['themes.js', 'entries.js', 'search-core.js', 'app.js', 'styles.css']) {
  chk(indexHtml.indexOf(f) >= 0, `引用了 ${f}`);
}
chk(indexHtml.indexOf('gallery.html') >= 0, '有对比页入口');

console.log('');
console.log(`主题 ${THEMES.length} 个，失败 ${fail} 项`);
process.exit(fail ? 1 : 0);
