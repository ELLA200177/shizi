/* 对比页：为每个主题嵌一个真实面板 */
(function () {
  'use strict';
  var THEMES = window.SHIZI_THEMES || [];
  var grid = document.getElementById('grid');
  var Q = encodeURIComponent('\u809D\u8C46');
  THEMES.forEach(function (t) {
    var url = 'index.html?embed=1&theme=' + t.id + '&q=' + Q;
    var card = document.createElement('div');
    card.className = 'card';
    var cap = document.createElement('div');
    cap.className = 'cap';
    cap.innerHTML = '<span class="nm"></span><span class="ds"></span>'
      + '<a class="open" target="_blank" href="' + url + '">单独打开 &#8599;</a>';
    cap.querySelector('.nm').textContent = t.name;
    cap.querySelector('.ds').textContent = t.desc;
    var fr = document.createElement('iframe');
    fr.src = url;
    fr.loading = 'lazy';
    fr.title = t.name;
    card.appendChild(cap);
    card.appendChild(fr);
    grid.appendChild(card);
  });
})();
