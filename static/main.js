// Минимальный JS: только то, что реально улучшает UX.

// Спойлеры: клик раскрывает.
document.addEventListener('click', function (e) {
  var sp = e.target.closest('.spoiler');
  if (sp && !sp.classList.contains('revealed')) {
    sp.classList.add('revealed');
    e.preventDefault();
  }
});

// Цитаты >>N: подсветка и прыжок к посту.
document.addEventListener('click', function (e) {
  var ref = e.target.closest('a.ref');
  if (!ref) return;
  var id = ref.getAttribute('data-ref');
  var post = document.getElementById('p' + id);
  if (post) {
    post.scrollIntoView({ behavior: 'smooth', block: 'center' });
    post.classList.add('highlighted');
    setTimeout(function () { post.classList.remove('highlighted'); }, 1500);
    e.preventDefault();
  }
});

// Быстрый ответ: префилл >>N в форму.
document.addEventListener('click', function (e) {
  var link = e.target.closest('.reply-link');
  if (!link) return;
  var ta = document.querySelector('.post-form textarea[name="body"]');
  if (ta) {
    var ref = link.getAttribute('data-ref');
    if (ta.value && !ta.value.endsWith('\n')) ta.value += '\n';
    ta.value += '>>' + ref + '\n';
    ta.focus();
  }
  e.preventDefault();
});

// Жалоба: показать/скрыть форму.
document.addEventListener('click', function (e) {
  var link = e.target.closest('.report-link');
  if (!link) return;
  var form = link.closest('.post').querySelector('.report-form');
  if (form) form.classList.toggle('hidden');
  e.preventDefault();
});
