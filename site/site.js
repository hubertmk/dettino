(function () {
  "use strict";
  var reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  var root = document.documentElement;

  /* ---------- downloads ---------- */
  var DL = window.DETTINO_DOWNLOADS || {};
  document.querySelectorAll("a[data-dl]").forEach(function (a) {
    var os = a.getAttribute("data-dl"), url = DL[os];
    if (url) { a.href = url; return; }
    a.href = "mailto:ciao@dettino.com?subject=" + encodeURIComponent("Tell me when Dettino for " + (os === "mac" ? "Mac" : "Windows") + " is out");
    a.lastChild.textContent = (os === "mac" ? "Mac" : "Windows") + ": tell me when it’s out";
  });

  var demo = document.getElementById("demo");
  if (demo) initDemo();
  var map = document.getElementById("map");
  if (map) initRoutes();

  /* =====================================================================
     The hero demo: the headline's keys, an app window, and the app's line
     at the foot of a small screen
     ===================================================================== */
  function initDemo() {
    /* ---------- the line, after the app's pill (apps/desktop/src/pill/engine.ts) ----------
       One stroke is the idle mark, the meter while you talk, the hump while it thinks and a green flash once the text is in.
       Drawn in the app's pixels (24 long at rest, 56 while talking) on a 170 × 36 canvas that CSS scales up. */
    var LW = 170, LH = 36, LY = 21, NP = 72;
    function Spring(x) { this.x = this.t = x; this.v = 0; this.k = 300; this.z = 1; }
    Spring.prototype.to = function (t, kz) { this.t = t; this.k = kz[0]; this.z = kz[1]; };
    Spring.prototype.step = function (dt) {
      if (reduce) { this.x = this.t; this.v = 0; return; }
      var c = 2 * this.z * Math.sqrt(this.k), h = dt / 4;
      for (var i = 0; i < 4; i++) { this.v += (this.k * (this.t - this.x) - c * this.v) * h; this.x += this.v * h; }
      if (Math.abs(this.x - this.t) < .02 && Math.abs(this.v) < .4) { this.x = this.t; this.v = 0; }
    };
    Spring.prototype.done = function () { return this.x === this.t && this.v === 0; };
    // Full length and spring per phase: entry overshoots once, the return to idle comes in a pixel short and relaxes out.
    var PH = { idle: [24, [300, .82]], listening: [56, [300, .72]], thinking: [44, [220, 1]], flash: [40, [300, .8]] };
    // A voice without a microphone: one burst per word of `n`, peaking just before the word shows (words land every 230 ms from 200 ms).
    function speech(t, n) {
      var j = Math.round((t - .12) / .23); if (j < 0 || j >= n) return 0;
      var d = (t - .12 - j * .23) / .23, loud = .65 + .35 * Math.sin(j * 2.3 + 1.7);
      return loud * Math.pow(Math.cos(d * Math.PI), 2) * (.85 + .15 * Math.sin(t * 29));
    }
    function rgb(h) { var n = parseInt(h.trim().slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; }
    function mix(a, b, t) { return a.map(function (v, i) { return v + (b[i] - v) * t; }); }
    function rgba(c, a) { return "rgba(" + (c[0] | 0) + "," + (c[1] | 0) + "," + (c[2] | 0) + "," + a + ")"; }
    function win(u, p) { return Math.pow(Math.cos((u - .5) * Math.PI), p); }

    function makeLine(cv) {
      var cs = getComputedStyle(root), pal = { ink: rgb(cs.getPropertyValue("--ink")), accent: rgb(cs.getPropertyValue("--voice")), ok: rgb(cs.getPropertyValue("--ok")) };
      var ctx = cv.getContext("2d"), phase = "idle", col = null, wl = [], lastEmit = 0, lv = 0, quiet = 0, talkT = 0, thinkT = 0, flashT = 0, said = 0, last = 0, running = false,
          L = new Spring(12), sw = new Spring(2), op = new Spring(0), th = new Spring(0), sp = [L, sw, op, th];
      // Speech leaves the centre as wavelets that travel outward at 58 px/s and fade.
      function ripple(x, t) {
        var ax = Math.abs(x), k = Math.min(1, L.x / 60), tau = .3 + .25 * k, amp = 8 * (.6 + .4 * k), boost = phase === "thinking" ? 6 : 0, y = 0;
        wl.forEach(function (w) {
          var age = t - w.b, d = 58 * age, g = Math.exp(-Math.pow((ax - d) / 8, 2)); if (age < 0 || (g < .01 && ax > d)) return;
          y += w.a * amp * Math.exp(-age / tau) * Math.exp(-boost * age) * g * Math.cos((ax - d) * .5);
        });
        return y;
      }
      function draw(t) {
        var s = cv.clientWidth / LW, z = s * (window.devicePixelRatio || 1), bw = Math.round(LW * z);
        if (!bw) return;
        if (cv.width !== bw) { cv.width = bw; cv.height = Math.round(LH * z); }
        ctx.setTransform(z, 0, 0, z, 0, 0); ctx.clearRect(0, 0, LW, LH);
        var cx = LW / 2, cy = LY - 2 * op.x, len = Math.max(1, L.x), h = th.x, hx = len * .6 * Math.sin(2 * Math.PI * .33 * (t - thinkT)), pts = [];
        if (lv > .04) quiet = t;
        var br = phase === "listening" && t - quiet > .7 ? .8 * Math.sin((t - quiet - .7) * 2 * Math.PI * .6) : 0;
        for (var i = 0; i <= NP; i++) {
          var u = i / NP, x = -len + 2 * len * u, w = win(u, 4), y = (wl.length ? ripple(x, t) * w : 0) + br * w;
          // thinking: a hump glides end to end on a line raised by 0.7 px
          if (h > .001) y -= (4.2 * Math.exp(-Math.pow((x - hx) / (len * .3), 2)) + .7) * win(u, .5) * h;
          pts.push([cx + x, cy + y]);
        }
        function path() { ctx.beginPath(); pts.forEach(function (p, i) { if (i) ctx.lineTo(p[0], p[1]); else ctx.moveTo(p[0], p[1]); }); }
        var c = col || pal.ink;
        ctx.lineCap = "round"; ctx.lineJoin = "round";
        // a light halo, then the line with a drop shadow once it has lifted
        ctx.save(); ctx.shadowColor = "rgba(255,255,255,.9)"; ctx.shadowBlur = 2.5 * z; path(); ctx.lineWidth = sw.x + .5; ctx.strokeStyle = "rgba(255,255,255,.55)"; ctx.stroke(); ctx.restore();
        // the flash glows green around the line and fades out over about a second
        var gl = phase === "flash" && !reduce ? Math.exp(-(t - flashT) / .45) : 0;
        if (gl > .01) { ctx.save(); ctx.shadowColor = rgba(pal.ok, .5 * gl); ctx.shadowBlur = 6 * gl * z; path(); ctx.lineWidth = sw.x + 2 * gl; ctx.strokeStyle = rgba(pal.ok, .18 * gl); ctx.stroke(); ctx.restore(); }
        ctx.save(); if (op.x > .01) { ctx.shadowColor = "rgba(0,0,0," + (.22 * op.x).toFixed(3) + ")"; ctx.shadowBlur = 4 * op.x * z; ctx.shadowOffsetY = 2 * op.x * z; }
        path(); ctx.lineWidth = sw.x; ctx.strokeStyle = rgba(c, phase === "idle" ? .75 : 1); ctx.stroke(); ctx.restore();
        // a glint rides the hump
        if (h > .02 && phase === "thinking" && !reduce) {
          var gx = cx + hx, gw = Math.max(10, len * .3), gr = ctx.createLinearGradient(gx - gw, 0, gx + gw, 0), hi = mix(c, [255, 255, 255], .7);
          gr.addColorStop(0, rgba(hi, 0)); gr.addColorStop(.5, rgba(hi, .55 * h)); gr.addColorStop(1, rgba(hi, 0)); path(); ctx.lineWidth = sw.x; ctx.strokeStyle = gr; ctx.stroke();
        }
      }
      // Runs only while something moves, as in the app.
      function tick(now) {
        var dt = Math.min(.05, last ? (now - last) / 1000 : .016), t = now / 1000; last = now;
        var raw = phase === "listening" ? speech(t - talkT, said) : 0; lv = reduce ? raw : lv + (raw - lv) * (1 - Math.exp(-dt * 16));
        // the flash: the stroke swells by 1 px for 200 ms as it turns green
        if (phase === "flash") sw.to(t - flashT < .2 ? 3.5 : 2.5, t - flashT < .2 ? [260, .7] : [300, .9]);
        sp.forEach(function (x) { x.step(dt); });
        if (phase === "listening" && lv > .06 && t - lastEmit > .07) { lastEmit = t; wl.push({ b: t, a: lv }); if (wl.length > 32) wl.shift(); }
        wl = wl.filter(function (w) { return t - w.b < 1.6; });
        var tc = phase === "flash" ? pal.ok : phase === "listening" ? pal.accent : phase === "thinking" ? mix(pal.accent, pal.ink, .25) : pal.ink;
        var cr = phase === "flash" ? 30 : 11;
        col = !col || reduce ? tc.slice() : col.map(function (v, i) { return v + (tc[i] - v) * (1 - Math.exp(-dt * cr)); });
        draw(t);
        if (phase !== "idle" || wl.length || !sp.every(function (x) { return x.done(); }) || col.some(function (v, i) { return Math.abs(v - pal.ink[i]) > .5; })) requestAnimationFrame(tick);
        else { running = false; last = 0; }
      }
      function wake() { if (!running) { running = true; requestAnimationFrame(tick); } }
      function set(p, n) {
        var t = performance.now() / 1000;
        if (p === "listening") { talkT = t; said = n || 0; }
        if (p === "thinking") thinkT = t;
        if (p === "flash") flashT = t;
        phase = p;
        L.to(PH[p][0] / 2, PH[p][1]); op.to(p === "idle" ? 0 : 1, [300, .9]); sw.to(p === "idle" ? 2 : 2.5, [300, .9]);
        th.to(p === "thinking" ? 1 : 0, p === "thinking" ? [400, 1] : [120, 1]); wake();
      }
      window.addEventListener("resize", wake);
      wake();
      return { set: set };
    }

    /* ---------- apps and example dictations ---------- */
    var ICON = {
      mail: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>',
      chat: '<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z"/>',
      code: '<path d="m8 8-4 4 4 4"/><path d="m16 8 4 4-4 4"/>',
      notes: '<path d="M6 3h9l4 4v14H6z"/><path d="M9 12h7M9 16h5"/>',
      doc: '<path d="M6 3h12v18H6z"/><path d="M9 8h6M9 12h6M9 16h3"/>'
    };
    // What the window already holds before the dictation lands: a greeting line, the other person's bubble, a heading, code.
    var PRE = {
      mail: '<i class="ph" style="width:32%"></i>',
      chat: '<span class="bub"></span>',
      doc: '<i class="ph" style="width:44%;height:12px"></i>',
      notes: '<i class="ph" style="width:58%"></i>',
      code: '<i class="ph" style="width:52%"></i><i class="ph" style="width:70%;margin-left:12%"></i>'
    };
    // A dictation as spoken: [said] keeps the word, [said, written] rewrites it, [said, ""] drops it.
    var SAMPLES = [
      { app: "mail", name: "Mail", ctx: "To: Lena", s: [["so", ""], ["um", ""], ["can", "Can"], ["we"], ["move"], ["the"], ["review"], ["to"], ["thursday", "Thursday"], ["ten thirty", "at 10:30?"], ["martin", "Martin"], ["cant", "can’t"], ["make"], ["wednesday", "Wednesday."]] },
      { app: "chat", name: "Chat", ctx: "Martin", s: [["uh", ""], ["the", "The"], ["drawings"], ["look"], ["like", ""], ["really"], ["good", "good,"], ["thanks", "thanks!"]] },
      { app: "doc", name: "Docs", ctx: "Pilot plan", s: [["the", "The"], ["pilot"], ["runs"], ["from"], ["march", "March"], ["first", "1"], ["to"], ["april", "April"], ["fifteenth", "15"], ["uh", ""], ["with"], ["twelve", "12"], ["people", "people."]] },
      { app: "notes", name: "Notes", ctx: "Today", s: [["okay", ""], ["so", ""], ["order", "Order"], ["two kilos", "2 kg"], ["of"], ["petg", "PETG"], ["before"], ["the"], ["fourteenth", "14th."]] },
      { app: "code", name: "Editor", ctx: "retry.ts", s: [["comment", "//"], ["retry", "Retry"], ["on"], ["four two nine", "429,"], ["at"], ["most"], ["three", "3"], ["times", "times."]] }
    ];
    function cleanText(s) { return s.map(function (tk) { return tk.length > 1 ? tk[1] : tk[0]; }).filter(Boolean).join(" "); }

    var app = document.getElementById("app"), hd = app.querySelector(".app-hd"), body = app.querySelector(".app-body"),
        keys = [].slice.call(document.querySelectorAll("[data-key]")), hold = document.getElementById("hold"), sr = document.getElementById("demo-sr"),
        line = makeLine(document.getElementById("line"));
    var say = null, st = "idle", owner = false, k = 0, cur = null, ws = [], shown = 0, ug = 0, ag = 0, visible = true;
    // Two clocks: U for the dictation in progress, A for the autoplay schedule, so a held key cancels one without the other.
    function U(ms, fn) { var g = ug; setTimeout(function () { if (g === ug) fn(); }, ms); }
    function A(ms, fn) { var g = ag; setTimeout(function () { if (g === ag) fn(); }, ms); }
    function el(t, c, h) { var e = document.createElement(t); if (c) e.className = c; if (h != null) e.innerHTML = h; return e; }

    function showApp(x) {
      hd.innerHTML = '<span><svg viewBox="0 0 24 24" aria-hidden="true">' + ICON[x.app] + "</svg>" + x.name + '</span><span class="ctx">' + (x.app === "chat" ? '<b class="av"></b>' : "") + x.ctx + "</span>";
      body.innerHTML = PRE[x.app];
      say = el("div", "say" + (x.app === "chat" ? " me" : x.app === "code" ? " code" : ""));
      body.appendChild(say);
    }
    function words(s, clean) {
      say.innerHTML = ""; say.classList.toggle("clean", !!clean);
      return s.map(function (tk) {
        var c = tk.length > 1 ? tk[1] : tk[0], w = el("span", "wd" + (clean ? " in" : ""));
        if (clean && c === "") w.className += " drop";
        w.textContent = clean && c ? c : tk[0]; say.appendChild(w); return w;
      });
    }
    function keysDown(on) { keys.forEach(function (x) { x.classList.toggle("down", on); }); }
    function describe() { sr.textContent = "Example: dictated to " + cur.name + ", " + cur.ctx + ". Pasted: " + cleanText(cur.s); }
    // On release: fillers are struck out and fold away, rewritten words get a seafoam highlight for a moment.
    function tidy(s) {
      s.forEach(function (tk, i) {
        if (tk.length < 2) return;
        var w = ws[i];
        U(i * 60, function () {
          if (tk[1] === "") { w.classList.add("strike"); U(reduce ? 0 : 420, function () { w.classList.add("drop"); }); }
          else { w.textContent = tk[1]; w.classList.add("fix"); U(850, function () { w.classList.remove("fix"); }); }
        });
      });
    }

    function press(byUser) {
      if (st !== "idle") return false;
      if (byUser) ag++;
      st = "listen"; owner = byUser; ug++;
      cur = SAMPLES[k++ % SAMPLES.length]; showApp(cur); app.classList.remove("out");
      ws = words(cur.s, false); shown = 0;
      keysDown(true); line.set("listening", ws.length);
      ws.forEach(function (w, i) { U(200 + i * 230, function () { if (st === "listen") { w.classList.add("in"); shown = i + 1; } }); });
      return true;
    }
    function release(byUser) {
      if (st !== "listen" || owner !== byUser) return;
      st = "think"; keysDown(false);
      var left = Math.max(0, ws.length - shown), s = cur.s;
      ws.forEach(function (w, i) { if (i >= shown) U((i - shown) * 35, function () { w.classList.add("in"); }); });
      line.set("thinking");
      var land = left * 35 + 900, done = land + s.length * 60 + 450;
      U(land, function () { tidy(s); line.set("flash"); });
      U(done, function () { say.classList.add("clean"); say.appendChild(el("span", "caret")); describe(); });
      U(done + 900, function () { line.set("idle"); st = "idle"; auto(byUser ? 6000 : 2400); });
    }
    // Plays by itself while the demo is on screen; waits while you are holding the keys or the tab is in the background.
    function auto(delay) {
      if (reduce) return;
      A(delay, function () {
        if (st !== "idle") return;
        if (!visible || document.hidden) { auto(1500); return; }
        app.classList.add("out");
        A(420, function () { if (press(false)) A(cur.s.length * 230 + 400, function () { release(false); }); });
      });
    }

    /* ---------- hold to talk: the headline keys, the keys under the screen on phones, or your own keyboard ---------- */
    var keyHeld = false, pointerHeld = false;
    function down() { press(true); }
    function up() { release(true); }
    keys.filter(function (x) { return !hold || !hold.contains(x); }).concat(hold ? [hold] : []).forEach(function (t) {
      t.addEventListener("pointerdown", function (e) {
        if (e.button !== 0) return;
        e.preventDefault();
        try { t.setPointerCapture(e.pointerId); } catch (_) {}
        pointerHeld = true; down();
      });
      function end() { if (pointerHeld) { pointerHeld = false; up(); } }
      t.addEventListener("pointerup", end);
      t.addEventListener("pointercancel", end);
      t.addEventListener("lostpointercapture", end);
      t.addEventListener("contextmenu", function (e) { e.preventDefault(); });
    });
    window.addEventListener("keydown", function (e) {
      if (e.code === "Space" && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
        e.preventDefault();
        if (!e.repeat && !keyHeld) { keyHeld = true; down(); }
      }
    });
    window.addEventListener("keyup", function (e) {
      if (keyHeld && (e.code === "Space" || e.key === "Control")) { keyHeld = false; up(); }
    });
    window.addEventListener("blur", function () { if (keyHeld) { keyHeld = false; up(); } });
    if (hold) {
      hold.addEventListener("keydown", function (e) {
        if (e.key === " " || e.key === "Enter") { e.preventDefault(); if (!e.repeat && !keyHeld) { keyHeld = true; down(); } }
      });
      hold.addEventListener("keyup", function (e) {
        if ((e.key === " " || e.key === "Enter") && keyHeld) { e.preventDefault(); keyHeld = false; up(); }
      });
      hold.addEventListener("click", function (e) { e.preventDefault(); });
    }
    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (es) { visible = es[0].isIntersecting; }, { threshold: 0.2 }).observe(demo);
    }

    /* ---------- first paint: the first example sits pasted (the HTML holds it too), then the rounds begin ---------- */
    cur = SAMPLES[0]; k = 1; showApp(cur); ws = words(cur.s, true); say.appendChild(el("span", "caret")); describe();
    auto(3200);
  }

  /* =====================================================================
     Route map: who hears you
     ===================================================================== */
  function initRoutes() {
    var NODES = { mic: [0, "L"], sL: [1, "L"], sC: [1, "C"], cL: [2, "L"], cC: [2, "C"], cur: [3, "L"] };
    var LAB = {
      mic: ["Microphone", "Warm, keeps the last 1.5 s"],
      sL: ["Speech to text", "Parakeet or Whisper"],
      sC: ["Speech to text", "OpenAI or any compatible server"],
      cL: ["Cleanup", "Ollama or LM Studio"],
      cC: ["Cleanup", "Anthropic, OpenAI, Perplexity or any compatible server"],
      cur: ["Your cursor", "Any app"]
    };
    var ROUTES = {
      private: { main: ["mic", "sL", "cL", "cur"] },
      balanced: { main: ["mic", "sL", "cC", "cur"], fb: ["mic", "sC", "cC"] },
      cloud: { main: ["mic", "sC", "cC", "cur"] }
    };
    var ORDER = ["private", "balanced", "cloud"], active = "private";
    var NS = "http://www.w3.org/2000/svg";

    function geo() {
      var W = map.clientWidth, vert = W < 640;
      var H = vert ? 660 : 430;
      var pos = {};
      Object.keys(NODES).forEach(function (k) {
        var c = NODES[k][0], z = NODES[k][1];
        if (vert) pos[k] = [z === "L" ? W * 0.11 : W * 0.6, H * [0.1, 0.37, 0.63, 0.9][c]];
        else pos[k] = [W * [0.07, 0.36, 0.64, 0.93][c], z === "L" ? 158 : 298];
      });
      return { W: W, H: H, vert: vert, pos: pos };
    }
    function seg(g, a, b) {
      var A = g.pos[a], B = g.pos[b], d;
      if (g.vert) {
        if (A[0] === B[0]) return "L" + B[0] + " " + B[1];
        var gap = B[1] - A[1], dd = Math.min(Math.abs(B[0] - A[0]), gap * 0.42), mid = (A[1] + B[1]) / 2;
        return "L" + A[0] + " " + (mid - dd / 2) + "L" + B[0] + " " + (mid + dd / 2) + "L" + B[0] + " " + B[1];
      }
      if (A[1] === B[1]) return "L" + B[0] + " " + B[1];
      var dy = Math.abs(B[1] - A[1]), dx = B[0] - A[0], run = Math.min(dy, dx * 0.6), m = (A[0] + B[0]) / 2;
      d = "L" + (m - run / 2) + " " + A[1] + "L" + (m + run / 2) + " " + B[1] + "L" + B[0] + " " + B[1];
      return d;
    }
    function pathFor(g, list) { var p = g.pos[list[0]], d = "M" + p[0] + " " + p[1]; for (var i = 1; i < list.length; i++) d += seg(g, list[i - 1], list[i]); return d; }
    function el(n, a) { var e = document.createElementNS(NS, n); for (var k in a) e.setAttribute(k, a[k]); return e; }

    var refs = {};
    function render() {
      var g = geo();
      map.innerHTML = "";
      map.style.height = g.H + "px";
      var svg = el("svg", { viewBox: "0 0 " + g.W + " " + g.H, width: g.W, height: g.H });
      var defs = el("defs", {});
      var pat = el("pattern", { id: "hatch", width: 9, height: 9, patternUnits: "userSpaceOnUse", patternTransform: "rotate(45)" });
      pat.appendChild(el("rect", { width: 9, height: 9, fill: "#F4EEE2" }));
      pat.appendChild(el("rect", { width: 3, height: 9, fill: "#E6DDCB" }));
      defs.appendChild(pat); svg.appendChild(defs);
      if (g.vert) {
        svg.appendChild(el("rect", { class: "zone-cloud", x: g.W * 0.5, y: 0, width: g.W * 0.5, height: g.H }));
        svg.appendChild(el("line", { class: "zone-line", x1: g.W * 0.5, y1: 0, x2: g.W * 0.5, y2: g.H }));
      } else {
        svg.appendChild(el("rect", { class: "zone-cloud", x: 0, y: 228, width: g.W, height: g.H - 228 }));
        svg.appendChild(el("line", { class: "zone-line", x1: 0, y1: 228, x2: g.W, y2: 228 }));
      }
      refs.lines = {};
      ORDER.forEach(function (r) { var p = el("path", { class: "ln", d: pathFor(g, ROUTES[r].main) }); svg.appendChild(p); refs.lines[r] = p; });
      refs.fb = el("path", { class: "ln fb", d: pathFor(g, ROUTES.balanced.fb), style: "display:none" });
      svg.appendChild(refs.fb);
      refs.nodes = {};
      Object.keys(NODES).forEach(function (k) { var p = g.pos[k], c = el("circle", { class: "st", cx: p[0], cy: p[1], r: 10 }); svg.appendChild(c); refs.nodes[k] = c; });
      map.appendChild(svg);

      var zl1 = document.createElement("span"), zl2 = document.createElement("span");
      zl1.className = zl2.className = "zl";
      zl1.textContent = "On this computer"; zl2.textContent = "In the cloud, with your key";
      if (g.vert) { zl1.style.cssText = "left:14px;top:14px"; zl2.style.cssText = "left:calc(50% + 14px);top:14px"; }
      else { zl1.style.cssText = "left:20px;top:18px"; zl2.style.cssText = "left:20px;bottom:18px"; }
      map.appendChild(zl1); map.appendChild(zl2);

      refs.labs = {};
      Object.keys(NODES).forEach(function (k) {
        var p = g.pos[k], z = NODES[k][1], c = NODES[k][0], d = document.createElement("div");
        d.className = "lab";
        d.innerHTML = "<b>" + LAB[k][0] + "</b><span>" + LAB[k][1] + "</span>";
        if (g.vert) {
          var right = z === "L" ? g.W * 0.5 - 12 : g.W - 12, left = p[0] + 22;
          d.style.cssText = "left:" + left + "px;top:" + (p[1] - 11) + "px;width:" + (right - left) + "px";
        } else {
          var wpx = Math.min(200, g.W * 0.24), x = p[0] - wpx / 2, align = "center";
          if (c === 0) { x = p[0] - 12; align = "left"; }
          if (c === 3) { x = p[0] - wpx + 12; align = "right"; }
          d.style.cssText = "left:" + x + "px;width:" + wpx + "px;text-align:" + align + ";" + (z === "L" ? "bottom:" + (g.H - p[1] + 24) + "px" : "top:" + (p[1] + 24) + "px");
        }
        map.appendChild(d); refs.labs[k] = d;
      });
      paint();
    }
    function paint() {
      var on = {}, r = ROUTES[active];
      r.main.forEach(function (k) { on[k] = 1; });
      if (r.fb) r.fb.forEach(function (k) { on[k] = on[k] || 2; });
      ORDER.forEach(function (k) { refs.lines[k].classList.toggle("on", k === active); });
      // the active line is drawn last, on top of the others
      var svg = refs.lines[active].parentNode;
      svg.appendChild(refs.lines[active]);
      refs.fb.style.display = r.fb ? "" : "none";
      refs.fb.classList.toggle("on", !!r.fb);
      svg.appendChild(refs.fb);
      Object.keys(refs.nodes).forEach(function (k) { svg.appendChild(refs.nodes[k]); refs.nodes[k].classList.toggle("on", on[k] === 1); refs.labs[k].classList.toggle("off", !on[k]); });
    }
    document.querySelectorAll("[data-route]").forEach(function (b) {
      b.addEventListener("click", function () {
        active = b.getAttribute("data-route");
        document.querySelectorAll("[data-route]").forEach(function (x) { x.setAttribute("aria-pressed", x === b ? "true" : "false"); });
        document.querySelectorAll(".route-say [data-for]").forEach(function (p) { p.hidden = p.getAttribute("data-for") !== active; });
        paint();
      });
    });
    render();
    var rz = 0, lastW = map.clientWidth;
    window.addEventListener("resize", function () { clearTimeout(rz); rz = setTimeout(function () { if (map.clientWidth !== lastW) { lastW = map.clientWidth; render(); } }, 150); });
  }
})();
