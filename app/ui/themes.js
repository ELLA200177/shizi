/* 拾字 · 主题清单
 * 单一事实来源：styles.css 里每个 body[data-theme="id"] 必须与本表一一对应，
 * 且 :root 里必须有对应的 --dot-<id>。由 app/tools/test_ui.js 校验。
 */
window.SHIZI_THEMES = [
  { id: 'minimal', name: '\u6781\u7B80\u767D', desc: '\u767D\u5361\u7247 \u00B7 \u5355\u4E00\u84DD\u70B9\u7F00' },
  { id: 'azure',   name: '\u851A\u84DD',     desc: '\u6DF1\u8272 \u00B7 \u84DD' },
  { id: 'violet',  name: '\u7D2B\u7F57\u5170', desc: '\u6DF1\u8272 \u00B7 \u7D2B' },
  { id: 'indigo',  name: '\u975B\u84DD',     desc: '\u6DF1\u8272 \u00B7 \u84DD\u7D2B' },
  { id: 'cyan',    name: '\u9752\u84DD',     desc: '\u6DF1\u8272 \u00B7 \u51B7\u9752' },
  { id: 'magenta', name: '\u7EEF\u7D2B',     desc: '\u6DF1\u8272 \u00B7 \u7D2B\u7C89' },
  { id: 'aurora',  name: '\u6781\u5149',     desc: '\u6DF1\u8272 \u00B7 \u4E09\u8272' }
];
