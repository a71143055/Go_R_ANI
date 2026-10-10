// 공통 유틸: 탭 전환, 서버 API 호출, 좌표 변환, 판 그리기, 스크린리더 안내, 키보드 커서
"use strict";

const COLS = "ABCDEFGHJKLMNOPQRST"; // I 제외 (바둑 표준 표기)

const Common = {
  think() {
    return document.getElementById("think").value;
  },

  // 서버 API 호출
  async api(path, payload = {}) {
    const res = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    let data;
    try { data = await res.json(); } catch { data = {}; }
    if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
    return data;
  },

  // AI 응답(ai 필드)을 해설 목록에 남긴다 (목록은 aria-live라 스크린리더가 읽는다)
  logAi(listId, ai, colorName) {
    if (!ai) return;
    if (ai.move) {
      const mv = ai.move.toLowerCase() === "pass" ? "패스" : ai.move;
      Common.log(listId, `<span class="mv">AI${colorName ? " " + colorName : ""} ${Common.escape(mv)}</span> ${ai.comment ? Common.escape(ai.comment) : ""}`);
    } else if (ai.comment) {
      Common.log(listId, Common.escape(ai.comment));
    }
  },

  // 스크린리더에 즉시 읽힐 문장
  announce(text) {
    const el = document.getElementById("sr-announce");
    el.textContent = "";
    setTimeout(() => { el.textContent = text; }, 50);
  },

  // 판 상태를 문장으로 (판 읽기)
  describeBoard(board, size) {
    const groups = { 1: [], 2: [] };
    board.forEach((c, i) => { if (c) groups[c].push(Common.toCoord(i % size, Math.floor(i / size), size)); });
    const part = (name, list) => list.length ? `${name} ${list.length}개: ${list.join(", ")}` : `${name} 없음`;
    return `${part("흑", groups[1])}. ${part("백", groups[2])}.`;
  },

  // (x, y) — y=0이 맨 위. 표기는 아래가 1행.
  toCoord(x, y, size) { return COLS[x] + (size - y); },
  fromCoord(str, size) {
    const m = /^([A-HJ-T])(\d{1,2})$/i.exec(String(str).trim());
    if (!m) return null;
    const x = COLS.indexOf(m[1].toUpperCase());
    const y = size - parseInt(m[2], 10);
    if (x < 0 || x >= size || y < 0 || y >= size) return null;
    return { x, y };
  },

  log(listId, html, isError = false) {
    const ol = document.getElementById(listId);
    const li = document.createElement("li");
    li.innerHTML = html;
    if (isError) li.classList.add("err");
    ol.appendChild(li);
    ol.scrollTop = ol.scrollHeight;
  },

  escape(s) {
    return String(s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  },

  // 격자형 판 공용 렌더러
  Grid: class {
    constructor(canvas, size, opts = {}) {
      this.canvas = canvas;
      this.ctx = canvas.getContext("2d");
      this.setSize(size);
      this.stars = opts.stars;
    }
    setSize(size) {
      this.size = size;
      const W = this.canvas.width;
      this.margin = W * 0.06;
      this.step = (W - 2 * this.margin) / (size - 1);
    }
    pos(i) { return this.margin + i * this.step; }
    pick(evt) {
      const r = this.canvas.getBoundingClientRect();
      const sx = (evt.clientX - r.left) * (this.canvas.width / r.width);
      const sy = (evt.clientY - r.top) * (this.canvas.height / r.height);
      const x = Math.round((sx - this.margin) / this.step);
      const y = Math.round((sy - this.margin) / this.step);
      if (x < 0 || y < 0 || x >= this.size || y >= this.size) return null;
      return { x, y };
    }
    drawBoard(starPoints) {
      const { ctx, size } = this;
      const W = this.canvas.width;
      ctx.fillStyle = "#dcb35c";
      ctx.fillRect(0, 0, W, W);
      ctx.strokeStyle = "#3b2a14";
      ctx.lineWidth = 1.2;
      for (let i = 0; i < size; i++) {
        ctx.beginPath(); ctx.moveTo(this.pos(0), this.pos(i)); ctx.lineTo(this.pos(size - 1), this.pos(i)); ctx.stroke();
        ctx.beginPath(); ctx.moveTo(this.pos(i), this.pos(0)); ctx.lineTo(this.pos(i), this.pos(size - 1)); ctx.stroke();
      }
      ctx.fillStyle = "#3b2a14";
      for (const [x, y] of starPoints) {
        ctx.beginPath(); ctx.arc(this.pos(x), this.pos(y), Math.max(3, this.step * 0.1), 0, Math.PI * 2); ctx.fill();
      }
      ctx.font = `${Math.round(this.margin * 0.38)}px sans-serif`;
      ctx.textAlign = "center"; ctx.textBaseline = "middle";
      for (let i = 0; i < size; i++) {
        ctx.fillText(COLS[i], this.pos(i), this.margin * 0.45);
        ctx.fillText(COLS[i], this.pos(i), W - this.margin * 0.45);
        ctx.fillText(String(size - i), this.margin * 0.45, this.pos(i));
        ctx.fillText(String(size - i), W - this.margin * 0.45, this.pos(i));
      }
    }
    drawStone(x, y, color, alpha = 1) {
      const { ctx } = this;
      const cx = this.pos(x), cy = this.pos(y), r = this.step * 0.47;
      ctx.save();
      ctx.globalAlpha = alpha;
      const g = ctx.createRadialGradient(cx - r * 0.35, cy - r * 0.35, r * 0.1, cx, cy, r);
      if (color === 1) { g.addColorStop(0, "#666"); g.addColorStop(1, "#111"); }
      else { g.addColorStop(0, "#fff"); g.addColorStop(1, "#cfcac0"); }
      ctx.fillStyle = g;
      ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = "rgba(0,0,0,.35)"; ctx.lineWidth = 1; ctx.stroke();
      ctx.restore();
    }
    drawMarker(x, y, stoneColor) {
      const { ctx } = this;
      ctx.strokeStyle = stoneColor === 1 ? "#fff" : "#000";
      ctx.lineWidth = 2;
      ctx.beginPath(); ctx.arc(this.pos(x), this.pos(y), this.step * 0.2, 0, Math.PI * 2); ctx.stroke();
    }
    drawCross(x, y) {
      const { ctx } = this;
      const cx = this.pos(x), cy = this.pos(y), d = this.step * 0.22;
      ctx.strokeStyle = "#c62828"; ctx.lineWidth = 2.5;
      ctx.beginPath(); ctx.moveTo(cx - d, cy - d); ctx.lineTo(cx + d, cy + d); ctx.moveTo(cx + d, cy - d); ctx.lineTo(cx - d, cy + d); ctx.stroke();
    }
  },
};

// 키보드 커서: 판(canvas)에서 방향키로 이동하고 Enter/Space로 착수한다.
// opts: { size(), cellText(x, y), onPlay(x, y), redraw() }
Common.Cursor = class {
  constructor(canvas, opts) {
    this.opts = opts;
    this.x = null;
    this.y = null;
    canvas.addEventListener("focus", () => {
      if (this.x == null) { const c = Math.floor(opts.size() / 2); this.x = c; this.y = c; }
      this.say();
      opts.redraw();
    });
    canvas.addEventListener("blur", () => opts.redraw());
    canvas.addEventListener("keydown", e => {
      const n = opts.size();
      const moves = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
      if (moves[e.key]) {
        e.preventDefault();
        const [dx, dy] = moves[e.key];
        this.x = Math.min(n - 1, Math.max(0, (this.x ?? 0) + dx));
        this.y = Math.min(n - 1, Math.max(0, (this.y ?? 0) + dy));
        this.say();
        opts.redraw();
      } else if ((e.key === "Enter" || e.key === " ") && this.x != null) {
        e.preventDefault();
        opts.onPlay(this.x, this.y);
      }
    });
    this.canvas = canvas;
  }
  active() { return document.activeElement === this.canvas && this.x != null; }
  say() { Common.announce(`${Common.toCoord(this.x, this.y, this.opts.size())} ${this.opts.cellText(this.x, this.y)}`); }
  draw(grid) {
    if (!this.active()) return;
    const ctx = grid.ctx, d = grid.step * 0.5;
    ctx.save();
    ctx.strokeStyle = "#1565c0"; ctx.lineWidth = 3;
    ctx.strokeRect(grid.pos(this.x) - d, grid.pos(this.y) - d, d * 2, d * 2);
    ctx.restore();
  }
};

// 탭 전환
function showTab(name) {
  document.querySelectorAll(".tabs button").forEach(b => {
    b.classList.toggle("active", b.dataset.tab === name);
    if (b.dataset.tab === name) b.setAttribute("aria-current", "page"); else b.removeAttribute("aria-current");
  });
  document.querySelectorAll(".tab").forEach(s => s.classList.toggle("active", s.id === "tab-" + name));
}
document.querySelectorAll(".tabs button").forEach(b => b.addEventListener("click", () => showTab(b.dataset.tab)));
document.querySelectorAll("[data-goto]").forEach(b => b.addEventListener("click", () => showTab(b.dataset.goto)));

// 생각 시간 기억
(() => {
  const sel = document.getElementById("think");
  try { const v = localStorage.getItem("think"); if (v) sel.value = v; } catch {}
  sel.addEventListener("change", () => { try { localStorage.setItem("think", sel.value); } catch {} });
})();

// 엔진 상태
fetch("/api/status").then(r => r.json()).then(s => {
  const el = document.getElementById("status");
  el.textContent = `AI 준비됨 (${s.engine}, ${s.threads}스레드)`;
  el.className = "status ok";
}).catch(() => {
  const el = document.getElementById("status");
  el.textContent = "서버 연결 실패"; el.className = "status err";
});
