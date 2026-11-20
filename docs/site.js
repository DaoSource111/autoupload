// AutoUpload site behaviour. No cookies, no network requests.
// localStorage keys (this device only): "au-lang" (en|es) and "au-theme" (light|dark).
(function () {
  var root = document.documentElement;
  function get(k) { try { return localStorage.getItem(k); } catch (e) { return null; } }
  function set(k, v) { try { localStorage.setItem(k, v); } catch (e) {} }

  // Language: remember an explicit choice; only the English home page follows it.
  document.addEventListener("click", function (e) {
    var a = e.target.closest && e.target.closest("a[data-lang]");
    if (a) set("au-lang", a.getAttribute("data-lang"));
  });
  var isHome = /(^|\/)(index\.html)?$/.test(location.pathname) && !/\/es\//.test(location.pathname);
  if (root.lang === "en" && isHome && get("au-lang") === "es" && location.search.indexOf("lang=en") === -1) {
    location.replace("es/index.html");
    return;
  }

  // Night / light view.
  function current() {
    return root.getAttribute("data-theme") ||
      (window.matchMedia && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  }
  function label(btn) {
    var dark = current() === "dark";
    btn.setAttribute("aria-label", btn.getAttribute(dark ? "data-to-light" : "data-to-dark"));
    btn.setAttribute("title", btn.getAttribute("aria-label"));
  }
  document.querySelectorAll("button.theme").forEach(function (btn) {
    label(btn);
    btn.addEventListener("click", function () {
      var next = current() === "dark" ? "light" : "dark";
      root.setAttribute("data-theme", next);
      set("au-theme", next);
      label(btn);
    });
  });

  var reduce = window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Highlighter: sweeps across literal statements as they come into view.
  var marks = document.querySelectorAll("mark.hl");
  if (reduce || !("IntersectionObserver" in window)) {
    marks.forEach(function (m) { m.classList.add("on"); });
  } else {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add("on"); io.unobserve(en.target); }
      });
    }, { rootMargin: "0px 0px -15% 0px" });
    marks.forEach(function (m) { io.observe(m); });
  }

  // The upload shot plays once when it comes into view, ends on its resolved frame, and can be replayed.
  var shot = document.querySelector(".shot");
  if (shot && !reduce) {
    var play = function () {
      shot.classList.remove("play");
      shot.querySelectorAll("*").forEach(function (el) {
        el.getAnimations && el.getAnimations().forEach(function (an) { an.currentTime = 0; });
      });
      void shot.offsetWidth;
      shot.classList.add("play");
    };
    if ("IntersectionObserver" in window) {
      var seen = new IntersectionObserver(function (entries) {
        if (entries[0].isIntersecting) { play(); seen.disconnect(); }
      }, { threshold: 0.6 });
      seen.observe(shot);
    } else { play(); }
    var btn = shot.querySelector(".replay");
    if (btn) btn.addEventListener("click", play);
  } else if (shot) {
    var b2 = shot.querySelector(".replay"); if (b2) b2.hidden = true;
  }

  // Contents rail: mark the section being read.
  var rail = document.querySelectorAll(".rail a");
  if (rail.length && "IntersectionObserver" in window) {
    var byId = {};
    rail.forEach(function (a) { byId[a.getAttribute("href").slice(1)] = a; });
    var spy = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting && byId[en.target.id]) {
          rail.forEach(function (a) { a.classList.remove("here"); });
          byId[en.target.id].classList.add("here");
        }
      });
    }, { rootMargin: "0px 0px -70% 0px" });
    document.querySelectorAll(".card h2[id]").forEach(function (h) { spy.observe(h); });
  }
})();
