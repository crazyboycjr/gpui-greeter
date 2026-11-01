// gpui-greeter's homepage: the theme toggle, clips that play while they are
// seen, and the copy buttons.

(function () {
  "use strict";
  var root = document.documentElement;
  var system = window.matchMedia("(prefers-color-scheme: dark)");

  // Light or dark. A choice that matches the system's goes back to following
  // the system, so nothing is stored for it.
  function systemTheme() { return system.matches ? "dark" : "light"; }
  function shownTheme() { return root.dataset.theme || systemTheme(); }
  var toggle = document.querySelector(".theme-toggle");
  function describe() {
    if (!toggle) return;
    var theme = shownTheme();
    var following = !root.dataset.theme;
    var other = theme === "dark" ? "light" : "dark";
    toggle.setAttribute("aria-pressed", theme === "dark" ? "true" : "false");
    toggle.setAttribute("aria-label", "Dark theme");
    toggle.title = (theme === "dark" ? "Dark" : "Light") + " theme" +
      (following ? ", as the system is" : ", chosen here") + ". Click for " + other + ".";
  }
  if (toggle) {
    toggle.addEventListener("click", function () {
      var next = shownTheme() === "dark" ? "light" : "dark";
      try {
        if (next === systemTheme()) {
          delete root.dataset.theme;
          localStorage.removeItem("gpui-greeter-theme");
        } else {
          root.dataset.theme = next;
          localStorage.setItem("gpui-greeter-theme", next);
        }
      } catch (e) {
        root.dataset.theme = next;
      }
      describe();
    });
  }
  if (system.addEventListener) system.addEventListener("change", describe);
  describe();

  // Each clip's poster loads as it comes near.
  var still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var videos = Array.prototype.slice.call(document.querySelectorAll("video.clip"));
  function poster(v) { if (v.dataset.poster && !v.poster) v.poster = v.dataset.poster; }
  if ("IntersectionObserver" in window) {
    var near = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) { poster(entry.target); near.unobserve(entry.target); }
      });
    }, { rootMargin: "800px 0px" });
    videos.forEach(function (v) { near.observe(v); });
  } else {
    videos.forEach(poster);
  }

  // Clips play while they are on screen. With motion to be reduced they
  // never start by themselves: they show their poster and their controls.
  if (still) {
    videos.forEach(function (v) { v.controls = true; });
  } else if ("IntersectionObserver" in window) {
    var seen = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        var v = entry.target;
        if (entry.isIntersecting) {
          if (v.preload === "none") v.preload = "auto";
          var playing = v.play();
          if (playing && playing.catch) playing.catch(function () { v.controls = true; });
        } else {
          v.pause();
        }
      });
    }, { threshold: 0.35 });
    videos.forEach(function (v) { seen.observe(v); });
  } else {
    videos.forEach(function (v) { v.controls = true; });
  }

  // Copy buttons: each copies the commands of the block in its card.
  document.querySelectorAll(".copy").forEach(function (button) {
    button.addEventListener("click", function () {
      var card = button.closest(".install-item");
      var code = card && card.querySelector("pre code");
      if (!code || !navigator.clipboard) return;
      navigator.clipboard.writeText(code.textContent).then(function () {
        button.textContent = "Copied";
        setTimeout(function () { button.textContent = "Copy"; }, 1600);
      }, function () {});
    });
  });
})();
