// 공통 유틸: 탭 전환, Claude API 호출, 좌표 변환, 판 그리기
"use strict";

const COLS = "ABCDEFGHJKLMNOPQRST"; // I 제외 (바둑 표준 표기)

const Common = {
  effort() {
    return document.getElementById("effort").value;
  },

  async askClaude(path, payload) {
    const res = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...payload, effort: payload.effort || Common.effort() }),
    });
    let data;
    try { data = await res.json(); } catch { data = {}; }
    if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
    return data;
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

  boardAscii(board, size) {
    const sym = [".", "X", "O"];
    const lines = [];
    const header = "   " + COLS.slice(0, size).split("").join(" ");
    lines.push(header);
    for (let y = 0; y < size; y++) {
      const n = String(size - y).padStart(2, " ");
      let row = n + " ";
      for (let x = 0; x < size; x++) row += sym[board[y * size + x]] + (x < size - 1 ? " " : "");
      lines.push(row + " " + n);
    }
    lines.push(header);
    return lines.join("\n");
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

// 탭 전환
function showTab(name) {
  document.querySelectorAll(".tabs button").forEach(b => b.classList.toggle("active", b.dataset.tab === name));
  document.querySelectorAll(".tab").forEach(s => s.classList.toggle("active", s.id === "tab-" + name));
}
document.querySelectorAll(".tabs button").forEach(b => b.addEventListener("click", () => showTab(b.dataset.tab)));
document.querySelectorAll("[data-goto]").forEach(b => b.addEventListener("click", () => showTab(b.dataset.goto)));

// 생각 깊이 기억
(() => {
  const sel = document.getElementById("effort");
  try { const v = localStorage.getItem("effort"); if (v) sel.value = v; } catch {}
  sel.addEventListener("change", () => { try { localStorage.setItem("effort", sel.value); } catch {} });
})();

// 서버/API 키 상태
fetch("/api/status").then(r => r.json()).then(s => {
  const el = document.getElementById("status");
  if (s.key_configured) { el.textContent = `AI: ${s.model} 연결됨`; el.className = "status ok"; }
  else { el.textContent = "API 키 없음 — .env 설정 필요"; el.className = "status err"; }
}).catch(() => {
  const el = document.getElementById("status");
  el.textContent = "서버 연결 실패"; el.className = "status err";
});
