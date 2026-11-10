// The site's few interactive parts: copying the connect command, sorting tables, filtering
// the news and the experience calculator. Every page works without it.
(function () {
  "use strict";

  // copy the connect command
  document.querySelectorAll("[data-copy]").forEach(function (button) {
    button.addEventListener("click", function () {
      var source = document.getElementById(button.getAttribute("data-copy"));
      if (!source || !navigator.clipboard) return;
      navigator.clipboard.writeText(source.textContent.trim()).then(function () {
        var was = button.textContent;
        button.textContent = "Copied";
        setTimeout(function () { button.textContent = was; }, 1600);
      });
    });
  });

  // sortable tables: a header sorts by its column, again to reverse
  document.querySelectorAll("table.sortable").forEach(function (table) {
    var heads = table.querySelectorAll("thead th");
    heads.forEach(function (th, col) {
      th.tabIndex = 0;
      var sort = function () {
        var asc = th.getAttribute("aria-sort") !== "ascending";
        heads.forEach(function (h) { h.removeAttribute("aria-sort"); });
        th.setAttribute("aria-sort", asc ? "ascending" : "descending");
        var numeric = th.getAttribute("data-type") === "num";
        var body = table.tBodies[0];
        var rows = Array.prototype.slice.call(body.rows);
        rows.sort(function (a, b) {
          var x = a.cells[col], y = b.cells[col];
          var p = x.getAttribute("data-sort") || x.textContent.trim();
          var q = y.getAttribute("data-sort") || y.textContent.trim();
          var r = numeric ? Number(p) - Number(q) : p.localeCompare(q);
          return asc ? r : -r;
        });
        rows.forEach(function (r) { body.appendChild(r); });
      };
      th.addEventListener("click", sort);
      th.addEventListener("keydown", function (e) {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); sort(); }
      });
    });
  });

  // the news filter
  var filters = document.querySelectorAll(".filter button");
  filters.forEach(function (button) {
    button.addEventListener("click", function () {
      var want = button.getAttribute("data-filter");
      filters.forEach(function (b) { b.setAttribute("aria-pressed", b === button ? "true" : "false"); });
      document.querySelectorAll(".news-list .news").forEach(function (item) {
        item.hidden = want !== "all" && item.getAttribute("data-kind") !== want;
      });
    });
  });

  // the experience calculator
  var cls = document.getElementById("calc-class");
  if (cls && window.SM_EXP) {
    var race = document.getElementById("calc-race");
    var from = document.getElementById("calc-from");
    var to = document.getElementById("calc-to");
    var rows = document.getElementById("calc-rows");
    var total = document.getElementById("calc-total");
    var fmt = function (n) { return n.toLocaleString("en-US"); };
    var clamp = function (v) { return Math.max(1, Math.min(window.SM_EXP.max, Math.floor(Number(v) || 1))); };
    var update = function () {
      var mod = Number(cls.value) + Number(race.value);
      var table = window.SM_EXP.tables[String(mod)];
      if (!table) return;
      var a = clamp(from.value), b = clamp(to.value);
      if (b < a) { var t = a; a = b; b = t; }
      var html = "";
      for (var level = a; level <= b; level++) {
        var need = table[level], step = level > 1 ? table[level] - table[level - 1] : 0;
        html += "<tr><td>" + level + "</td><td>" + fmt(need) + "</td><td>" + fmt(step) + "</td></tr>";
      }
      rows.innerHTML = html;
      var sign = mod > 0 ? "+" + mod : (mod < 0 ? "−" + (-mod) : "±" + 0);
      total.textContent = cls.options[cls.selectedIndex].text + " " + race.options[race.selectedIndex].text +
        ": " + sign + "% experience. Level " + b + " takes " + fmt(table[b]) + " in all.";
    };
    [cls, race, from, to].forEach(function (el) { el.addEventListener("input", update); });
    update();
  }
})();
