// 알까기 화면: 물리 시뮬레이션은 서버(Rust 엔진)가 계산해 프레임으로 보내고, 여기서는 재생만 한다.
"use strict";
(() => {
  const $ = id => document.getElementById(id);
  const canvas = $("alk-canvas");
  const ctx = canvas.getContext("2d");
  const W = canvas.width;
  const M = W * 0.08;            // 판 바깥 여백 (떨어지는 돌이 보이도록)
  const S = W - 2 * M;           // 판 한 변 (픽셀)
  const R = 0.03;                // 돌 반지름 (판 단위) — 엔진과 동일
  const TRAVEL_MAX = 1.6;        // 힘 1.0일 때 미끄러지는 거리 — 엔진과 동일
  const LOG = "alk-log";

  let view = null, busy = false, aim = null, animFrame = null;
  const px = v => M + v * S;
  const humanTurn = () => view && !view.over && !busy && view.state.to_play === view.humanColor;

  function toBoard(evt) {
    const r = canvas.getBoundingClientRect();
    return {
      x: ((evt.clientX - r.left) * (W / r.width) - M) / S,
      y: ((evt.clientY - r.top) * (W / r.height) - M) / S,
    };
  }

  function drawStone(x, y, color, alpha = 1, label) {
    const cx = px(x), cy = px(y), r = R * S;
    ctx.save();
    ctx.globalAlpha = alpha;
    const g = ctx.createRadialGradient(cx - r * 0.35, cy - r * 0.35, r * 0.1, cx, cy, r);
    if (color === 1) { g.addColorStop(0, "#666"); g.addColorStop(1, "#111"); }
    else { g.addColorStop(0, "#fff"); g.addColorStop(1, "#cfcac0"); }
    ctx.fillStyle = g;
    ctx.beginPath(); ctx.arc(cx, cy, r, 0, Math.PI * 2); ctx.fill();
    ctx.strokeStyle = "rgba(0,0,0,.35)"; ctx.lineWidth = 1; ctx.stroke();
    if (label != null) {
      ctx.fillStyle = color === 1 ? "#ddd" : "#555";
      ctx.font = `${Math.round(r * 0.9)}px sans-serif`;
      ctx.textAlign = "center"; ctx.textBaseline = "middle";
      ctx.fillText(label, cx, cy + 1);
    }
    ctx.restore();
  }

  function draw(positions) {
    ctx.fillStyle = getComputedStyle(document.body).getPropertyValue("--bg") || "#f4efe6";
    ctx.fillRect(0, 0, W, W);
    ctx.fillStyle = "#dcb35c";
    ctx.fillRect(M, M, S, S);
    ctx.strokeStyle = "rgba(59,42,20,.6)"; ctx.lineWidth = 1;
    for (let i = 0; i < 19; i++) {
      const v = M + (S * (i + 0.5)) / 19;
      ctx.beginPath(); ctx.moveTo(M + S / 38, v); ctx.lineTo(W - M - S / 38, v); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(v, M + S / 38); ctx.lineTo(v, W - M - S / 38); ctx.stroke();
    }
    if (!view) return;
    const stones = view.state.stones;
    stones.forEach((s, i) => {
      const [x, y, alive] = positions ? positions[i] : [s.x, s.y, s.alive ? 1 : 0];
      if (alive) drawStone(x, y, s.color, 1, s.id);
      else if (positions && on(x, y, 0.15)) drawStone(x, y, s.color, 0.35); // 떨어지는 중
    });

    // 조준선
    if (aim) {
      const s = stones.find(st => st.id === aim.stone);
      const len = TRAVEL_MAX * aim.power * aim.power;
      ctx.save();
      ctx.strokeStyle = "rgba(198,40,40,.85)"; ctx.lineWidth = 3; ctx.setLineDash([8, 6]);
      ctx.beginPath(); ctx.moveTo(px(s.x), px(s.y));
      ctx.lineTo(px(s.x + Math.cos(aim.angle) * len), px(s.y + Math.sin(aim.angle) * len));
      ctx.stroke();
      ctx.restore();
      ctx.fillStyle = "#c62828"; ctx.font = "bold 18px sans-serif"; ctx.textAlign = "left";
      ctx.fillText(`힘 ${(aim.power * 100).toFixed(0)}%`, px(s.x) + 20, px(s.y) - 20);
    }
  }
  const on = (x, y, pad) => x > -pad && x < 1 + pad && y > -pad && y < 1 + pad;

  function play(frames) {
    return new Promise(resolve => {
      if (!frames?.length) return resolve();
      const start = performance.now();
      const step = now => {
        const k = Math.min(frames.length - 1, Math.floor((now - start) / (1000 / 30)));
        draw(frames[k]);
        if (k < frames.length - 1) animFrame = requestAnimationFrame(step);
        else resolve();
      };
      animFrame = requestAnimationFrame(step);
    });
  }

  function render(thinking) {
    draw();
    const info = $("alk-info");
    if (!view) { info.textContent = "새 게임을 눌러 시작하세요."; return; }
    const count = c => view.state.stones.filter(s => s.alive && s.color === c).length;
    let text = `남은 돌 — 나(흑) ${count(1)} · Claude(백) ${count(2)}\n`;
    if (view.result) text += `🏁 ${view.result}`;
    else if (thinking) text += thinking;
    else text += humanTurn() ? "내 차례입니다. 돌을 당겼다 놓으세요." : "Claude 차례입니다.";
    info.innerHTML = Common.escape(text).replace(/(Claude[^\n]*중)$/, '<span class="thinking">$1</span>');
  }

  async function shoot(stone, angle, power) {
    busy = true;
    render("발사!");
    try {
      const prev = view;
      view = await Common.api(`/api/alkkagi/${prev.id}/shoot`, { stone, angle, power });
      await play(view.frames);
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      busy = false;
      render();
      return;
    }
    busy = false;
    render();
    if (!view.over) await aiTurn();
  }

  async function aiTurn() {
    busy = true;
    render("Claude가 조준하는 중");
    try {
      view = await Common.api(`/api/alkkagi/${view.id}/ai`, { effort: Common.effort() });
      Common.logAi(LOG, view.ai);
      await play(view.frames);
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
    } finally {
      busy = false;
      render();
    }
  }

  canvas.addEventListener("pointerdown", e => {
    if (!humanTurn()) return;
    const p = toBoard(e);
    const s = view.state.stones.find(st => st.alive && st.color === view.humanColor && Math.hypot(st.x - p.x, st.y - p.y) < R * 1.6);
    if (!s) return;
    canvas.setPointerCapture(e.pointerId);
    aim = { stone: s.id, angle: 0, power: 0 };
  });
  canvas.addEventListener("pointermove", e => {
    if (!aim) return;
    const p = toBoard(e);
    const s = view.state.stones.find(st => st.id === aim.stone);
    const dx = s.x - p.x, dy = s.y - p.y;  // 당긴 반대 방향으로 발사
    aim.angle = Math.atan2(dy, dx);
    aim.power = Math.min(1, Math.hypot(dx, dy) / 0.35);
    draw();
  });
  canvas.addEventListener("pointerup", () => {
    if (!aim) return;
    const a = aim;
    aim = null;
    if (a.power < 0.05) { draw(); return; }
    shoot(a.stone, a.angle, a.power);
  });
  canvas.addEventListener("pointercancel", () => { aim = null; draw(); });

  $("alk-new").addEventListener("click", async () => {
    if (busy) return;
    cancelAnimationFrame(animFrame);
    try {
      view = await Common.api("/api/alkkagi/new", { count: parseInt($("alk-count").value, 10) });
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      return;
    }
    $(LOG).innerHTML = "";
    render();
  });

  render();
})();
