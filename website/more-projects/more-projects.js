/* "Check my other projects" section. Self-contained and reusable.
 *
 * To use on another site:
 *   1. Copy this whole folder (more-projects/) anywhere on that site.
 *   2. Put <div data-more-projects data-current="OpenLocalDictation"></div> where
 *      the section should appear.
 *   3. Add <script src="path/to/more-projects/more-projects.js" defer></script>.
 *
 * data-current  repo name of the site it is on; that project is left out of the list.
 * data-title    optional heading override.
 *
 * Navigation drop-down (same list, icon on the left, name on the right): put
 *   <div data-projects-menu data-current="OpenLocalDictation"></div>
 * (or an <li> with the same attributes) inside the site's nav. data-label sets
 * the button text (default "My projects"). The menu opens on click and closes
 * on Escape, on a click elsewhere, or when focus leaves it.
 *
 * Styles are injected by this file and read the host site's CSS variables
 * (--bg, --surface, --fg, --fg-muted, --primary, --line, --radius-lg, --sans)
 * with dark fallbacks, so it works on a site that defines none of them.
 * Icons are resolved next to this script, so the folder can be moved freely.
 *
 * To add a project: append one object to PROJECTS and drop its webp icon in
 * this folder. Keep GitHub-hosted names exact. */
(function () {
  "use strict";

  var PROJECTS = [
    {
      repo: "OpenLocalServer",
      name: "Open Local Server",
      blurb:
        "Local dev environment manager for Windows: runtimes, sites, databases, trusted HTTPS, Docker and an MCP server.",
      icon: "open-local-server.webp",
      site: "https://kz370.github.io/OpenLocalServer/",
      github: "https://github.com/kz370/OpenLocalServer",
    },
    {
      repo: "OpenLocalDictation",
      name: "Open Local Dictation",
      blurb:
        "Hold a key, speak, and the text appears in the app you were using. Whisper runs on your PC; nothing leaves it.",
      icon: "open-local-dictation.webp",
      site: "https://kz370.github.io/OpenLocalDictation/",
      github: "https://github.com/kz370/OpenLocalDictation",
    },
    {
      repo: "OpenLocalAssistant",
      name: "Open Local Assistant",
      blurb:
        "Free, private desktop AI assistant for Windows. Chat, voice, hands-free calls and dictation, local with LM Studio or hosted.",
      icon: "open-local-assistant.webp",
      site: "https://kz370.github.io/OpenLocalAssistant/",
      github: "https://github.com/kz370/OpenLocalAssistant",
    },
  ];

  var script = document.currentScript;
  var base = script && script.src ? script.src.replace(/[^/]*$/, "") : "";

  var CSS = [
    ".omp{padding-block:clamp(48px,6vw,72px)}",
    ".omp__wrap{width:min(100% - 48px,var(--maxw,1180px));margin-inline:auto;font-family:var(--sans,system-ui,sans-serif);color:var(--fg,#f1f3f3)}",
    ".omp__eyebrow{display:inline-flex;align-items:center;gap:8px;font-size:12px;font-weight:600;letter-spacing:.1em;text-transform:uppercase;color:var(--primary,#5fd4b0);margin-bottom:14px}",
    ".omp__eyebrow::before{content:'';width:6px;height:6px;border-radius:2px;background:currentColor}",
    ".omp__title{margin:0;font-size:clamp(24px,3vw,32px);line-height:1.2;letter-spacing:-.02em}",
    ".omp__lead{margin:12px 0 0;max-width:60ch;color:var(--fg-muted,#a3adad);font-size:16px}",
    ".omp__grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr));gap:18px;margin-top:clamp(28px,4vw,40px)}",
    ".omp__card{display:flex;flex-direction:column;gap:14px;padding:22px;border:1px solid var(--line,rgba(255,255,255,.1));border-radius:var(--radius-lg,16px);background:var(--surface,#1e2323);transition:border-color .22s,transform .22s}",
    ".omp__card:hover{border-color:var(--primary,#5fd4b0);transform:translateY(-3px)}",
    ".omp__head{display:flex;align-items:center;gap:14px}",
    ".omp__icon{width:48px;height:48px;border-radius:12px;flex:none;object-fit:contain}",
    ".omp__name{margin:0;font-size:17px;letter-spacing:-.02em}",
    ".omp__blurb{margin:0;color:var(--fg-muted,#a3adad);font-size:14.5px;line-height:1.55;flex:1}",
    ".omp__links{display:flex;flex-wrap:wrap;gap:10px}",
    ".omp__link{display:inline-flex;align-items:center;justify-content:center;width:40px;height:40px;border-radius:10px;border:1px solid var(--line,rgba(255,255,255,.16));color:var(--fg,#f1f3f3);text-decoration:none}",
    ".omp__link:hover{border-color:var(--primary,#5fd4b0);color:var(--primary,#5fd4b0)}",
    ".omp__link:focus-visible{outline:2px solid var(--primary,#5fd4b0);outline-offset:2px}",
    "@media (prefers-reduced-motion:reduce){.omp__card{transition:none}.omp__card:hover{transform:none}}",
    ".omp-menu{position:relative;display:inline-block;font-family:var(--sans,system-ui,sans-serif);list-style:none}",
    ".omp-menu__btn{display:inline-flex;align-items:center;gap:6px;margin:0;padding:7px 11px;border:0;border-radius:7px;background:none;color:var(--fg-muted,#a3adad);font:inherit;font-size:15px;font-weight:500;line-height:inherit;cursor:pointer}",
    ".omp-menu__btn:hover,.omp-menu__btn[aria-expanded=true]{color:var(--fg,#f1f3f3)}",
    ".omp-menu__btn:focus-visible,.omp-menu .omp-menu__item:focus-visible{outline:2px solid var(--primary,#5fd4b0);outline-offset:2px}",
    ".omp-menu__chev{width:14px;height:14px;transition:transform .18s}",
    ".omp-menu__btn[aria-expanded=true] .omp-menu__chev{transform:rotate(180deg)}",
    ".omp-menu__panel{position:absolute;right:0;top:calc(100% + 8px);z-index:60;min-width:250px;padding:6px;border:1px solid var(--line,rgba(255,255,255,.12));border-radius:var(--radius-lg,14px);background:var(--surface,#1e2323);box-shadow:0 18px 44px rgba(0,0,0,.5)}",
    ".omp-menu__panel[hidden]{display:none}",
    ".omp-menu .omp-menu__item{display:flex;align-items:center;gap:12px;padding:9px 10px;border-radius:10px;color:var(--fg,#f1f3f3);font-size:15px;font-weight:600;line-height:1.3;text-decoration:none;white-space:nowrap}",
    ".omp-menu .omp-menu__item:hover{background:rgba(255,255,255,.06);color:var(--primary,#5fd4b0)}",
    ".omp-menu__icon{width:28px;height:28px;border-radius:7px;flex:none;object-fit:contain}",
    "@media (max-width:860px){.omp-menu{display:block}.omp-menu__panel{position:static;margin-top:4px;box-shadow:none;min-width:0}}",
    "@media (prefers-reduced-motion:reduce){.omp-menu__chev{transition:none}}",
  ].join("\n");

  function el(tag, cls, text) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text) node.textContent = text;
    return node;
  }

  var ICONS = {
    website:
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.8 5.6 3.8 9s-1.3 6.4-3.8 9c-2.5-2.6-3.8-5.6-3.8-9S9.5 5.6 12 3z"/></svg>',
    github:
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true"><path d="M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.88-1.37-3.88-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.28 1.19-3.09-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.83 1.19 3.09 0 4.42-2.69 5.39-5.25 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5z"/></svg>',
  };

  function link(href, label, icon) {
    var a = el("a", "omp__link");
    a.innerHTML = ICONS[icon]; /* constant markup above, no page data */
    a.href = href;
    a.title = label;
    a.setAttribute("aria-label", label);
    a.target = "_blank";
    a.rel = "noopener noreferrer";
    return a;
  }

  function render(mount) {
    var current = (mount.dataset.current || "").toLowerCase();
    var list = PROJECTS.filter(function (p) {
      return p.repo.toLowerCase() !== current;
    });
    if (!list.length) return;

    var wrap = el("div", "omp__wrap");
    wrap.appendChild(el("div", "omp__eyebrow", "More from the same author"));
    wrap.appendChild(
      el("h2", "omp__title", mount.dataset.title || "Check my other projects")
    );
    wrap.appendChild(
      el(
        "p",
        "omp__lead",
        "Free, open-source Windows apps that run on your own PC."
      )
    );

    var grid = el("div", "omp__grid");
    list.forEach(function (p) {
      var card = el("article", "omp__card");
      var head = el("div", "omp__head");
      var img = el("img", "omp__icon");
      img.src = base + p.icon;
      img.alt = "";
      img.width = 48;
      img.height = 48;
      img.loading = "lazy";
      head.appendChild(img);
      head.appendChild(el("h3", "omp__name", p.name));
      card.appendChild(head);
      card.appendChild(el("p", "omp__blurb", p.blurb));
      var links = el("div", "omp__links");
      if (p.site) links.appendChild(link(p.site, p.name + " website", "website"));
      links.appendChild(link(p.github, p.name + " on GitHub", "github"));
      card.appendChild(links);
      grid.appendChild(card);
    });
    wrap.appendChild(grid);

    mount.classList.add("omp");
    mount.appendChild(wrap);
  }

  function renderMenu(mount) {
    var current = (mount.dataset.current || "").toLowerCase();
    var list = PROJECTS.filter(function (p) {
      return p.repo.toLowerCase() !== current;
    });
    if (!list.length) return;

    var id = "omp-menu-" + (renderMenu.n = (renderMenu.n || 0) + 1);
    mount.classList.add("omp-menu");

    var btn = el("button", "omp-menu__btn", mount.dataset.label || "My projects");
    btn.type = "button";
    btn.setAttribute("aria-expanded", "false");
    btn.setAttribute("aria-controls", id);
    var NS = "http://www.w3.org/2000/svg";
    var chev = document.createElementNS(NS, "svg");
    chev.setAttribute("class", "omp-menu__chev");
    chev.setAttribute("viewBox", "0 0 24 24");
    chev.setAttribute("fill", "none");
    chev.setAttribute("stroke", "currentColor");
    chev.setAttribute("stroke-width", "2");
    chev.setAttribute("stroke-linecap", "round");
    chev.setAttribute("stroke-linejoin", "round");
    chev.setAttribute("aria-hidden", "true");
    var path = document.createElementNS(NS, "path");
    path.setAttribute("d", "m6 9 6 6 6-6");
    chev.appendChild(path);
    btn.appendChild(chev);

    var panel = el("div", "omp-menu__panel");
    panel.id = id;
    panel.hidden = true;
    list.forEach(function (p) {
      var a = el("a", "omp-menu__item");
      a.href = p.site || p.github;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      var img = el("img", "omp-menu__icon");
      img.src = base + p.icon;
      img.alt = "";
      img.width = 28;
      img.height = 28;
      a.appendChild(img);
      a.appendChild(el("span", "", p.name));
      panel.appendChild(a);
    });

    function set(open) {
      panel.hidden = !open;
      btn.setAttribute("aria-expanded", open ? "true" : "false");
    }
    btn.addEventListener("click", function () {
      set(panel.hidden);
    });
    document.addEventListener("click", function (e) {
      if (!mount.contains(e.target)) set(false);
    });
    mount.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && !panel.hidden) {
        set(false);
        btn.focus();
      }
    });
    mount.addEventListener("focusout", function (e) {
      if (e.relatedTarget && !mount.contains(e.relatedTarget)) set(false);
    });

    mount.appendChild(btn);
    mount.appendChild(panel);
  }

  function init() {
    var mounts = document.querySelectorAll("[data-more-projects]");
    var menus = document.querySelectorAll("[data-projects-menu]");
    if (!mounts.length && !menus.length) return;
    var style = document.createElement("style");
    style.textContent = CSS;
    document.head.appendChild(style);
    Array.prototype.forEach.call(mounts, render);
    Array.prototype.forEach.call(menus, renderMenu);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
