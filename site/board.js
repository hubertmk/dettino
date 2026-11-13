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

  var board = document.getElementById("board");
  if (board) initBoard();
  var map = document.getElementById("map");
  if (map) initRoutes();

  /* =====================================================================
     The departures board
     ===================================================================== */
  function initBoard() {
    var DUR = 76;
    var GLYPHS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:?";
    var IS_MAC = root.getAttribute("data-os") === "mac";

    var ICON = {
      mail: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>',
      chat: '<path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z"/>',
      code: '<path d="m8 8-4 4 4 4"/><path d="m16 8 4 4-4 4"/>',
      notes: '<path d="M6 3h9l4 4v14H6z"/><path d="M9 12h7M9 16h5"/>',
      doc: '<path d="M6 3h12v18H6z"/><path d="M9 8h6M9 12h6M9 16h3"/>'
    };
    // A dictation as spoken: [said] keeps the word, [said, written] rewrites it, [said, ""] drops it.
    var SAMPLES = [
      { app: "mail", name: "Mail", ctx: "To Lena", s: [["so", ""], ["um", ""], ["can", "Can"], ["we"], ["move"], ["the"], ["review"], ["to"], ["thursday", "Thursday"], ["ten thirty", "at 10:30?"], ["martin", "Martin"], ["cant", "can’t"], ["make"], ["wednesday", "Wednesday."]] },
      { app: "chat", name: "Chat", ctx: "Martin", s: [["uh", ""], ["the", "The"], ["drawings"], ["look"], ["like", ""], ["really"], ["good", "good,"], ["thanks", "thanks!"]] },
      { app: "doc", name: "Docs", ctx: "Pilot plan", s: [["the", "The"], ["pilot"], ["runs"], ["from"], ["march", "March"], ["first", "1"], ["to"], ["april", "April"], ["fifteenth", "15"], ["uh", ""], ["with"], ["twelve", "12"], ["people", "people."]] },
      { app: "notes", name: "Notes", ctx: "Today", s: [["okay", ""], ["so", ""], ["order", "Order"], ["two kilos", "2 kg"], ["of"], ["petg", "PETG"], ["before"], ["the"], ["fourteenth", "14th."]] },
      { app: "code", name: "Editor", ctx: "retry.ts", s: [["comment", "//"], ["retry", "Retry"], ["on"], ["four two nine", "429,"], ["at"], ["most"], ["three", "3"], ["times", "times."]] },
      { app: "notes", name: "Notes", ctx: "To do", s: [["note", "Note"], ["to"], ["self", "self:"], ["um", ""], ["call"], ["the"], ["tax"], ["office"], ["about"], ["the"], ["q three", "Q3"], ["return", "return."]] }
    ];
    SAMPLES.forEach(function (x) { var st = strings(x.s); x.raw = st.raw; x.clean = st.clean; x.rr = st.rr; x.cr = st.cr; });

    function strings(s) {
      var raw = "", clean = "", rr = [], cr = [];
      s.forEach(function (tk) {
        var said = tk[0], w = tk.length > 1 ? tk[1] : tk[0], kind = tk.length < 2 ? "" : (w === "" ? "drop" : "chg");
        if (raw) raw += " ";
        rr.push([raw.length, raw.length + said.length, kind]);
        raw += said;
        if (w) {
          if (clean) clean += " ";
          cr.push([clean.length, clean.length + w.length, kind]);
          clean += w;
        }
      });
      return { raw: raw, clean: clean, rr: rr, cr: cr };
    }

    /* ---------- flap cells ---------- */
    function mkCell() {
      var el = document.createElement("span");
      el.className = "c";
      el.innerHTML = '<span class="t"><b></b></span><span class="b"><b></b></span><span class="lt"><b></b></span><span class="lb"><b></b></span>';
      var b = el.querySelectorAll("b");
      return { el: el, t: b[0], b: b[1], lt: b[2], lb: b[3], ltE: el.children[2], lbE: el.children[3], ch: " ", target: " ", extra: 0, running: false, gen: 0 };
    }
    function vis(ch) { return ch === " " ? "" : ch; }
    function setNow(cell, ch) {
      cell.gen++; cell.running = false; cell.extra = 0; cell.ch = cell.target = ch;
      var v = vis(ch);
      cell.t.textContent = cell.b.textContent = cell.lt.textContent = cell.lb.textContent = v;
    }
    function flipTo(cell, ch, steps, delay) {
      cell.target = ch;
      cell.extra = Math.max(cell.extra, (steps || 1) - 1);
      if (reduce) { setNow(cell, ch); return; }
      if (cell.running) return;
      if (cell.ch === ch && cell.extra <= 0) return;
      cell.running = true;
      var g = cell.gen;
      if (delay) setTimeout(function () { if (g === cell.gen) run(cell); }, delay); else run(cell);
    }
    function run(cell) {
      if (cell.ch === cell.target && cell.extra <= 0) { cell.running = false; return; }
      var n;
      if (cell.extra > 0) { cell.extra--; n = GLYPHS.charAt(Math.floor(Math.random() * GLYPHS.length)); }
      else n = cell.target;
      flipOnce(cell, n, function () { run(cell); });
    }
    function flipOnce(cell, n, done) {
      var g = cell.gen, ov = vis(cell.ch), nv = vis(n);
      cell.t.textContent = nv; cell.lt.textContent = ov; cell.b.textContent = ov; cell.lb.textContent = nv;
      var a = cell.ltE.animate([{ transform: "rotateX(0deg)" }, { transform: "rotateX(-90deg)" }], { duration: DUR / 2, easing: "cubic-bezier(.55,0,1,.45)" });
      a.onfinish = function () {
        if (g !== cell.gen) return;
        cell.lt.textContent = nv;
        var b = cell.lbE.animate([{ transform: "rotateX(90deg)" }, { transform: "rotateX(0deg)" }], { duration: DUR / 2, easing: "cubic-bezier(0,.55,.45,1)" });
        b.onfinish = function () {
          if (g !== cell.gen) return;
          cell.b.textContent = nv; cell.ch = n; done();
        };
      };
    }

    /* ---------- grids of cells ---------- */
    function makeGrid(host, lines, cols) {
      host.innerHTML = "";
      var cells = [];
      for (var l = 0; l < lines; l++) {
        var fl = document.createElement("span");
        fl.className = "fl";
        for (var c = 0; c < cols; c++) { var cell = mkCell(); fl.appendChild(cell.el); cells.push(cell); }
        host.appendChild(fl);
      }
      return { host: host, lines: lines, cols: cols, cells: cells, map: [], text: "" };
    }
    // Greedy word wrap; map[i] is the cell index of character i (or -1 when it falls off the grid).
    function wrap(text, cols) {
      var map = new Array(text.length), line = 0, col = 0, re = /\S+/g, m;
      for (var i = 0; i < text.length; i++) map[i] = -1;
      while ((m = re.exec(text))) {
        var w = m[0];
        if (col > 0 && col + 1 + w.length > cols) { line++; col = 0; } else if (col > 0) col++;
        for (var j = 0; j < w.length; j++) {
          if (col >= cols) { line++; col = 0; }
          map[m.index + j] = line * cols + col; col++;
        }
      }
      return { map: map, lines: text.trim() ? line + 1 : 0 };
    }
    function show(grid, text, o) {
      o = o || {};
      var w = wrap(text, grid.cols), n = grid.cells.length, arr = new Array(n), i;
      for (i = 0; i < n; i++) arr[i] = " ";
      for (i = 0; i < text.length; i++) if (w.map[i] >= 0 && w.map[i] < n) arr[w.map[i]] = text.charAt(i);
      if (o.trunc && w.lines > grid.lines) {
        var last = n - 1;
        while (last > 0 && arr[last] === " ") last--;
        arr[Math.min(n - 1, last + 1)] = "…";
        if (last + 1 >= n) arr[n - 1] = "…";
      }
      grid.map = w.map; grid.text = text;
      for (i = 0; i < n; i++) {
        var cell = grid.cells[i], ch = arr[i];
        if (o.now) { setNow(cell, ch); continue; }
        var steps = typeof o.steps === "function" ? o.steps() : (o.steps || 1);
        if (ch === " " && cell.ch === " ") continue;
        flipTo(cell, ch, cell.ch === ch ? 1 : steps, (o.stagger || 0) * i + (o.jitter ? Math.random() * o.jitter : 0));
      }
      return w;
    }
    function cellsFor(grid, ranges, kind) {
      var out = [];
      ranges.forEach(function (r) {
        if (kind && r[2] !== kind) return;
        for (var i = r[0]; i < r[1]; i++) { var k = grid.map[i]; if (k >= 0 && grid.cells[k]) out.push(grid.cells[k]); }
      });
      return out;
    }
    function cssPx(name) { return parseFloat(getComputedStyle(board).getPropertyValue(name)) || 0; }
    function hhmm(d) { d = d || new Date(); return ("0" + d.getHours()).slice(-2) + ":" + ("0" + d.getMinutes()).slice(-2); }
    function minsAgo(m) { return hhmm(new Date(Date.now() - m * 60000)); }

    /* ---------- build the board ---------- */
    var live = document.getElementById("live"), histHost = document.getElementById("hist"), liveSr = document.getElementById("live-sr");
    var L = {
      time: live.querySelector('[data-k="time"]'), to: live.querySelector('[data-k="to"]'),
      status: live.querySelector('[data-k="status"]'), text: live.querySelector('[data-k="text"]')
    };
    L.to.classList.add("swap");
    var G = { time: makeGrid(L.time, 1, 5), status: makeGrid(L.status, 1, 9), text: null };
    var clock = makeGrid(document.getElementById("clock"), 1, 5);

    var hist = [
      { x: SAMPLES[3], t: minsAgo(4), said: false },
      { x: SAMPLES[4], t: minsAgo(9), said: false },
      { x: SAMPLES[5], t: minsAgo(17), said: false }
    ];
    var rows = hist.map(function (h, i) {
      var b = document.createElement("button");
      b.type = "button"; b.className = "row hist"; b.setAttribute("aria-pressed", "false");
      b.innerHTML = '<span class="flaps sig" aria-hidden="true"></span><span class="to swap"></span><span class="flaps col-text" aria-hidden="true"></span><span class="flaps" aria-hidden="true"></span>';
      histHost.appendChild(b);
      var r = { el: b, time: makeGrid(b.children[0], 1, 5), to: b.children[1], textHost: b.children[2], status: makeGrid(b.children[3], 1, 9), text: null };
      b.addEventListener("click", function () { toggleSaid(i); });
      return r;
    });

    function toHTML(x) { return '<svg viewBox="0 0 24 24" aria-hidden="true">' + ICON[x.app] + "</svg><b>" + x.name + "</b><span>" + x.ctx + "</span>"; }
    function swapHTML(el, html, now) {
      if (now || reduce) { el.innerHTML = html; el.classList.remove("out"); return; }
      el.classList.add("out");
      setTimeout(function () { el.innerHTML = html; el.classList.remove("out"); }, 200);
    }
    function rowLabel(h) {
      return "Example dictation to " + h.x.name + ", " + h.x.ctx + ", at " + h.t + ". " + (h.said ? "What was said: " + h.x.raw : "Pasted: " + h.x.clean) + ". Press to show " + (h.said ? "the cleaned text." : "what was said.");
    }
    function renderRow(i, now) {
      var r = rows[i], h = hist[i], o = now ? { now: true } : { stagger: 7, steps: 2 };
      show(r.time, h.t, o);
      show(r.text, h.said ? h.x.raw : h.x.clean, { now: o.now, stagger: o.stagger, steps: o.steps, trunc: true });
      show(r.status, h.said ? "As said" : "Pasted", o);
      swapHTML(r.to, toHTML(h.x), now);
      r.el.setAttribute("aria-pressed", h.said ? "true" : "false");
      r.el.setAttribute("aria-label", rowLabel(h));
    }
    function toggleSaid(i) { hist[i].said = !hist[i].said; renderRow(i); }

    // Grid sizes follow the layout: the live row gets as many lines as the longest example needs.
    var liveCols = 0, liveLines = 0, histCols = 0, cur = SAMPLES[0];
    function layoutGrids() {
      var cw = cssPx("--cw"), cg = cssPx("--cg"), sw = cssPx("--sw"), sg = cssPx("--sg");
      var lw = live.querySelector(".live-text").clientWidth;
      var cols = Math.max(12, Math.floor((lw + cg) / (cw + cg)));
      var lines = 2;
      SAMPLES.forEach(function (x) { lines = Math.max(lines, wrap(x.raw, cols).lines, wrap(x.clean, cols).lines); });
      lines = Math.max(lines, wrap("Hold Ctrl + Space and talk.", cols).lines);
      if (cols !== liveCols || lines !== liveLines) {
        liveCols = cols; liveLines = lines;
        var keep = G.text ? G.text.text : "";
        var dim = G.text && G.text.cells[0] && G.text.cells[0].el.classList.contains("dim");
        G.text = makeGrid(L.text, lines, cols);
        show(G.text, keep, { now: true });
        if (dim) setDim(true);
      }
      var tw = rows[0].textHost.parentNode ? rows[0].textHost.clientWidth : 0;
      // the text column is display:inline-grid, so measure the grid track instead
      var track = rows[0].el.getBoundingClientRect().width;
      var cs = getComputedStyle(rows[0].el).gridTemplateColumns.split(" ").map(parseFloat);
      if (cs.length >= 3) tw = cs[2]; else if (cs.length === 2) tw = cs[1];
      if (!tw) tw = track * 0.5;
      var hc = Math.max(10, Math.floor((tw + sg) / (sw + sg)));
      if (hc !== histCols) {
        histCols = hc;
        rows.forEach(function (r, i) { r.text = makeGrid(r.textHost, 1, hc); renderRow(i, true); });
      }
    }

    /* ---------- the line (the app's pill) ---------- */
    var pill = (function () {
      var path = document.querySelector("#pill path"), wrapEl = document.getElementById("pill-wrap"), W = 240, H = 40, MID = W / 2;
      var st = "idle", w = 40, tw = 40, amp = 0, tamp = 0, flash = 0, raf = 0, t0 = performance.now(), color = "", sw = "";
      // Same colours as the app's line: accent while listening, green when the text is pasted.
      var COL = { idle: "#C4C8D0", listen: "var(--line-live)", think: "var(--board-label)", done: "var(--success)" };
      var LEN = { idle: 40, listen: 210, think: 150, done: 120 };
      function draw(t) {
        w += (tw - w) * 0.16; amp += (tamp - amp) * 0.14;
        var x0 = MID - w / 2, d = "", n = Math.max(2, Math.round(w / 3));
        for (var i = 0; i <= n; i++) {
          var u = i / n, x = x0 + u * w, y = H / 2;
          var env = Math.sin(Math.PI * u);
          if (amp > 0.01) {
            var voice = 0.55 + 0.45 * Math.sin(t * 0.0043) * Math.sin(t * 0.0017 + 1.3);
            y += amp * env * voice * (Math.sin(x * 0.16 + t * 0.013) * 9 + Math.sin(x * 0.06 - t * 0.007) * 5);
          }
          if (st === "think") { var pos = ((t - t0) / 900) % 1; y -= 8 * Math.exp(-Math.pow(u - pos, 2) / 0.004) * env; }
          d += (i ? "L" : "M") + x.toFixed(1) + " " + y.toFixed(1);
        }
        path.setAttribute("d", d);
        var c = COL[st];
        if (c !== color) { color = c; path.style.stroke = c; }
        // On paste the stroke swells once, like the app's flash, then settles.
        var nsw = (4 + 3 * flash).toFixed(2);
        if (nsw !== sw) { sw = nsw; path.style.strokeWidth = nsw; }
        flash *= 0.9;
        var settled = Math.abs(tw - w) < 0.3 && amp < 0.01 && st === "idle" && flash < 0.02;
        raf = settled ? 0 : requestAnimationFrame(draw);
      }
      function kick() { if (!raf) raf = requestAnimationFrame(draw); }
      function set(s) {
        st = s; t0 = performance.now();
        tw = LEN[s];
        wrapEl.classList.toggle("listen", s === "listen");
        wrapEl.classList.toggle("done", s === "done");
        tamp = s === "listen" ? 1 : 0;
        if (s === "done") flash = 1;
        if (reduce) { w = tw; amp = 0; flash = 0; draw(t0); cancelAnimationFrame(raf); raf = 0; return; }
        kick();
      }
      draw(t0);
      return { set: set };
    })();

    /* ---------- the live cycle ---------- */
    var GEN = 0, state = "idle", idx = 1, shown = 0, pressT = 0, manual = false, lastUser = -1e9, visible = true, cascadeAt = 0;
    var WORD = 290;
    function after(ms, fn) { var g = GEN; setTimeout(function () { if (g === GEN) fn(); }, ms); }
    function setDim(on) { G.text.cells.forEach(function (c) { c.el.classList.toggle("dim", on); }); }
    function clearMarks() { G.text.cells.forEach(function (c) { c.el.classList.remove("drop", "hl"); }); }
    function setMeta(x) {
      if (x) swapHTML(L.to, toHTML(x));
      else swapHTML(L.to, "");
    }
    function idle(msg) {
      state = "idle"; manual = false;
      board.classList.remove("pressed");
      clearMarks(); setMeta(null);
      G.status.cells.forEach(function (c) { c.el.classList.remove("ok"); });
      show(G.text, msg || "Hold Ctrl + Space and talk.", { stagger: 6, steps: 2 });
      setDim(true);
      show(G.status, "Ready", { stagger: 20 });
      pill.set("idle");
    }
    function startListen(byUser) {
      GEN++; state = "listen"; manual = byUser; shown = 0; pressT = performance.now();
      cur = SAMPLES[idx % SAMPLES.length]; idx++;
      board.classList.add("pressed");
      clearMarks(); setDim(false);
      setMeta(cur);
      show(G.time, hhmm(), { stagger: 20 });
      show(G.status, "Listening", { stagger: 20 });
      show(G.text, "", { stagger: 2 });
      pill.set("listen");
      var words = cur.raw.split(" ");
      function tick() {
        if (state !== "listen") return;
        shown++;
        show(G.text, words.slice(0, shown).join(" "));
        if (shown < words.length) after(WORD * (0.75 + Math.random() * 0.6), tick);
        else if (!manual) after(520, release);
      }
      after(byUser ? 380 : 480, tick);
    }
    function release() {
      if (state !== "listen") return;
      var held = performance.now() - pressT;
      board.classList.remove("pressed");
      if (shown === 0) {
        GEN++;
        idle(held < 250 ? "A quick tap does what Ctrl + Space always did. Hold it a little longer." : "Didn’t hear anything. Hold, then talk.");
        schedule(4200);
        return;
      }
      state = "clean";
      show(G.text, cur.raw, { stagger: 3 });
      pill.set("think");
      show(G.status, "Cleaning", { stagger: 20 });
      after(reduce ? 0 : 420, function () { cellsFor(G.text, cur.rr, "drop").forEach(function (c) { c.el.classList.add("drop"); }); });
      after(reduce ? 0 : 1050, cascade);
    }
    function cascade() {
      clearMarks();
      var n = G.text.cells.length, stagger = 8, jitter = 40;
      show(G.text, cur.clean, { stagger: stagger, jitter: jitter, steps: function () { return 2 + Math.floor(Math.random() * 3); } });
      var total = reduce ? 0 : n * stagger + jitter + 5 * DUR + 60;
      after(total, function () {
        var hl = cellsFor(G.text, cur.cr, "chg");
        hl.forEach(function (c) { c.el.classList.add("hl"); });
        after(1300, function () { hl.forEach(function (c) { c.el.classList.remove("hl"); }); });
        show(G.status, "Pasted", { stagger: 20 });
        G.status.cells.forEach(function (c) { c.el.classList.add("ok"); });
        after(1600, function () { G.status.cells.forEach(function (c) { c.el.classList.remove("ok"); }); });
        pill.set("done");
        state = "done";
        liveSr.textContent = "Example: dictated to " + cur.name + ", " + cur.ctx + ". Pasted: " + cur.clean;
        setTimeout(function () { if (state === "done") pill.set("idle"); }, 1600);
        after(3000, depart);
      });
    }
    function depart() {
      hist.unshift({ x: cur, t: G.time.text || hhmm(), said: false });
      hist.pop();
      hist.forEach(function (h) { h.said = false; });
      rows.forEach(function (r, i) { renderRow(i); });
      idle();
      schedule(2200);
    }
    function schedule(ms) {
      if (reduce) return;
      after(ms, function () {
        if (!visible || document.hidden || performance.now() - lastUser < 7000) { schedule(1500); return; }
        startListen(false);
      });
    }

    function userDown() {
      lastUser = performance.now();
      board.classList.add("used");
      if (state === "listen" && manual) return;
      startListen(true);
    }
    function userUp() {
      lastUser = performance.now();
      if (state === "listen" && manual) release();
    }

    var keyHeld = false;
    window.addEventListener("keydown", function (e) {
      if (e.code === "Space" && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
        e.preventDefault();
        if (!e.repeat && !keyHeld) { keyHeld = true; userDown(); }
      }
    });
    window.addEventListener("keyup", function (e) {
      if (keyHeld && (e.code === "Space" || e.key === "Control")) { keyHeld = false; userUp(); }
    });
    window.addEventListener("blur", function () { if (keyHeld) { keyHeld = false; userUp(); } });

    var hold = document.getElementById("hold"), pointerHeld = false;
    hold.addEventListener("pointerdown", function (e) {
      if (e.button !== 0) return;
      e.preventDefault();
      try { hold.setPointerCapture(e.pointerId); } catch (_) {}
      pointerHeld = true; userDown();
    });
    function pointerEnd() { if (pointerHeld) { pointerHeld = false; userUp(); } }
    hold.addEventListener("pointerup", pointerEnd);
    hold.addEventListener("pointercancel", pointerEnd);
    hold.addEventListener("lostpointercapture", pointerEnd);
    hold.addEventListener("contextmenu", function (e) { e.preventDefault(); });
    hold.addEventListener("keydown", function (e) {
      if (e.key === " " || e.key === "Enter") { e.preventDefault(); if (!e.repeat && !keyHeld) { keyHeld = true; userDown(); } }
    });
    hold.addEventListener("keyup", function (e) {
      if ((e.key === " " || e.key === "Enter") && keyHeld) { e.preventDefault(); keyHeld = false; userUp(); }
    });
    hold.addEventListener("click", function (e) { e.preventDefault(); });

    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (es) { visible = es[0].isIntersecting; }, { threshold: 0.2 }).observe(board);
    }

    /* ---------- clock ---------- */
    function tickClock() { show(clock, hhmm(), { stagger: 30, steps: 2 }); }
    show(clock, hhmm(), { now: true });
    setInterval(function () { if (clock.text !== hhmm()) tickClock(); }, 5000);

    /* ---------- first paint: complete at rest, then the board starts its rounds ---------- */
    layoutGrids();
    show(G.time, minsAgo(0), { now: true });
    show(G.status, "Pasted", { now: true });
    setMeta(cur);
    swapHTML(L.to, toHTML(cur), true);
    liveSr.textContent = "Example: dictated to " + cur.name + ", " + cur.ctx + ". Pasted: " + cur.clean;
    if (reduce) {
      show(G.text, cur.clean, { now: true });
    } else {
      show(G.text, cur.clean, { stagger: 9, jitter: 40, steps: function () { return 2 + Math.floor(Math.random() * 4); } });
      state = "done";
      after(4200, depart);
    }
    if (reduce) state = "done";

    var rz = 0;
    window.addEventListener("resize", function () { clearTimeout(rz); rz = setTimeout(layoutGrids, 150); });
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
      pat.appendChild(el("rect", { width: 9, height: 9, fill: "#EEF1F5" }));
      pat.appendChild(el("rect", { width: 3, height: 9, fill: "#E0E5EC" }));
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
