(function () {
  "use strict";
  var forms = document.querySelectorAll("form.danger[data-confirm]");
  forms.forEach(function (f) {
    f.addEventListener("submit", function (e) {
      if (!window.confirm(f.getAttribute("data-confirm"))) {
        e.preventDefault();
      }
    });
  });
})();
