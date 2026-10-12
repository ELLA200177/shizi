/* 拾字 · 检索核心回归测试
 * 运行： node app/tools/test_search.js
 */
'use strict';
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const uiDir = path.join(__dirname, '..', 'ui');
const sandbox = { window: {} };
vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(path.join(uiDir, 'entries.js'), 'utf8'), sandbox, { filename: 'entries.js' });
const entries = sandbox.window.SHIZI_ENTRIES;
const S = require(path.join(uiDir, 'search-core.js'));
const index = S.buildIndex(entries);

let fail = 0;

// ---------- A. 归一化单测 ----------
const U = (c) => String.fromCodePoint(c);
const CANON = [
  [U(0x03B4) + U(0x00B9) + U(0x00B3) + 'C', 'd13c'.replace('d', U(0x03B4)), 'delta+superscript'],
  ['H' + U(0x2082) + 'O', 'h2o', 'H2O 下标归一'],
  ['n' + U(0x01D0) + ' h' + U(0x01CE) + 'o', 'nihao', '声调剥离'],
  [U(0x00B5), U(0x03BC), '微符号 -> 希腊 mu'],
  ['\uff28', 'h', '全角 -> 半角'],
  ['(A123' + U(0x00D7) + 'B456)', 'a123' + U(0x00D7) + 'b456', '括号剥离'],
  ['\u2014\u2014', '\u2014\u2014', '破折号保留（可精确搜到）'],
];
console.log('--- A. 归一化 ---');
for (const [input, want, name] of CANON) {
  const got = S.canonical(input);
  const ok = got === want;
  if (!ok) fail++;
  console.log(`  ${ok ? 'OK  ' : 'FAIL'} ${name}  canonical=${JSON.stringify(got)} want=${JSON.stringify(want)}`);
}

// ---------- B. 不可见字符 ----------
console.log('--- B. 不可见字符显示 ---');
for (const id of ['language.zh.0032', 'language.zh.0033', 'language.zh.0034']) {
  const e = entries.find(x => x.id === id);
  const label = S.displayLabel(e);
  const ok = S.isInvisibleOnly(e.text) && label.indexOf('U+') > 0;
  if (!ok) fail++;
  console.log(`  ${ok ? 'OK  ' : 'FAIL'} ${id}  标签=${label}`);
}

// ---------- C. 检索 Top-1 ----------
const CASES = [
  ['13c', 'chemistry.zh.0023', '上下标数字归一'],
  ['\u03B4\u00B9\u00B3C', 'chemistry.zh.0023', '原样输入'],
  ['h2o', 'chemistry.zh.0001', '化学式'],
  ['h2so4', 'chemistry.zh.0009', '化学式'],
  ['h2s', 'chemistry.zh.0009', '前缀'],
  ['caco3', 'chemistry.zh.0010', '化学式'],
  ['xila', 'chemistry.zh.0057', '希腊表别名'],
  ['cheng', 'chemistry.zh.0061', '\u4E58\u53F7'],
  ['\u4E58\u53F7', 'chemistry.zh.0061', '\u4E58\u53F7中文'],
  ['miao', 'chemistry.zh.0043', '\u89D2\u79D2'],
  ['nihao', 'language.zh.0007', '拼音词'],
  ['nbsp', 'language.zh.0032', '不可见空格'],
  ['zwsp', 'language.zh.0033', '零宽空格'],
  ['5-3', 'chemistry.zh.0059', '核酸方向'],
  ['\u71DA', 'language.zh.0048', '生僻字单字（燚）'],
  ['\u9F98', 'language.zh.0046', '生僻字单字（龘）'],
  ['\u767C', 'language.zh.0042', '中文子串（發 / 髮）'],
  ['bpmf',  'language.zh.0015', '注音别名'],
  ['lvse',  'language.zh.0013', '带 ü 的拼音'],
];
console.log('--- C. 检索 Top-1 ---');
for (const [q, wantId, name] of CASES) {
  const res = S.search(index, q, 3);
  const top = res[0] && res[0].entry;
  const ok = top && top.id === wantId;
  if (!ok) fail++;
  const topShow = top ? `${top.id} ${JSON.stringify(S.displayLabel(top).slice(0, 24))}` : '(无结果)';
  const inTop3 = res.some(r => r.entry.id === wantId);
  console.log(`  ${ok ? 'OK  ' : (inTop3 ? 'TOP3' : 'FAIL')} ${JSON.stringify(q).padEnd(22)} -> ${topShow}${ok ? '' : '   期望 ' + wantId}`);
}

// 注意：医学包移出后，词库里已没有任何条目带 initialism 字段，
// 所以「拼音首字母」这条检索路径暂时没有数据可测。算法本身保留了，
// 等日后加入带中文术语的领域包会自动重新覆盖。

// ---------- F. 空查询（面板打开时的初始列表）----------
console.log('--- F. 空查询 ---');
{
  const top10 = S.search(index, '', 10);
  const shapeOk = top10.length === 10 && top10.every(x => x && x.entry && x.entry.id);
  if (!shapeOk) fail++;
  console.log(`  ${shapeOk ? 'OK  ' : 'FAIL'} 空查询返回同构结果（10 条，每条含 .entry）`);

  const all = S.search(index, '', 0);
  const allOk = all.length === entries.length && all.every(x => x && x.entry);
  if (!allOk) fail++;
  console.log(`  ${allOk ? 'OK  ' : 'FAIL'} 不传 limit 时返回全部 ${entries.length} 条`);

  const blank = S.search(index, '   ', 10);
  const blankOk = blank.length === 10 && blank.every(x => x && x.entry);
  if (!blankOk) fail++;
  console.log(`  ${blankOk ? 'OK  ' : 'FAIL'} 纯空白查询也走空查询分支`);
}

// ---------- E. 希腊字母（24 个各自独立成条，各有触发码）----------
console.log('--- E. \u5e0c\u814a\u5b57\u6bcd ---');
{
  const NAMES = ['alpha','beta','gamma','delta','epsilon','zeta','eta','theta','iota','kappa',
                 'lambda','mu','nu','xi','omicron','pi','rho','sigma','tau','upsilon',
                 'phi','chi','psi','omega'];
  let bad = 0, noTrig = [], sample = null;
  NAMES.forEach((name, i) => {
    const id = 'chemistry.zh.' + String(63 + i).padStart(4, '0');
    const e = entries.find(x => x.id === id);
    if (!e) { bad++; if (!sample) sample = id + ' 不存在'; return; }
    if (!e.trigger) noTrig.push(name);
    const top = S.search(index, name, 2)[0];
    if (!top || top.entry.id !== id) {
      bad++;
      if (!sample) sample = `${name} 期望 ${id}，实际 ${top && top.entry.id}`;
    }
  });
  if (bad) fail++;
  console.log(`  ${bad === 0 ? 'OK  ' : 'FAIL'} 24 \u4e2a\u5b57\u6bcd\u6309\u82f1\u6587\u540d\u641c\u7d22\u5747\u547d\u4e2d\u81ea\u8eab${sample ? '  ' + sample : ''}`);
  const ok2 = noTrig.length === 0;
  if (!ok2) fail++;
  console.log(`  ${ok2 ? 'OK  ' : 'FAIL'} 24 \u4e2a\u5b57\u6bcd\u5747\u6709\u89e6\u53d1\u7801${noTrig.length ? '  \u7f3a: ' + noTrig.join(',') : ''}`);
}

// ---------- D. 触发词 ----------
console.log('--- D. 触发词 ---');
{
  const trigs = entries.filter(e => e.trigger);
  const uniq = new Set(trigs.map(e => e.trigger)).size === trigs.length;
  if (!uniq) fail++;
  console.log(`  ${uniq ? 'OK  ' : 'FAIL'} ${trigs.length} 个触发词全局唯一`);

  let bad = 0, sample = null;
  for (const e of trigs) {
    const res = S.search(index, e.trigger.replace(/^;/, ''), 3);
    if (!res.some(r => r.entry.id === e.id)) {
      bad++;
      if (!sample) sample = `${e.trigger} 期望 ${e.id}，实际 ${res[0] && res[0].entry.id}`;
    }
  }
  if (bad) fail++;
  console.log(`  ${bad === 0 ? 'OK  ' : 'FAIL'} ${trigs.length} 个触发词都能搜到自己${sample ? '  ' + sample : ''}`);

  console.log(`  INFO 无触发词 ${entries.filter(e => !e.trigger).length} 条（符号表类，只能搜索）`);
}

console.log('');
console.log(`词库 ${entries.length} 条，用例 ${CANON.length + 3 + CASES.length} 个，失败 ${fail} 个`);
process.exit(fail ? 1 : 0);
