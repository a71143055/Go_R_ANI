// 오목 화면: 승패 판정과 AI는 서버(Rust 엔진)가 맡는다.
"use strict";
(() => {
  const $ = id => document.getElementById(id);
  const canvas = $("omok-canvas");
  const grid = new Common.Grid(canvas, 15);
  const STARS = [[3, 3], [11, 3], [7, 7], [3, 11], [11, 11]];
  const LOG = "omok-log";
  const NAME = { 1: "흑", 2: "백" };

  let view = null, busy = false, hover = null;
  const humanTurn = () => view && !view.over && !busy && view.state.to_play === view.humanColor;

  const cursor = new Common.Cursor(canvas, {
    size: () => 15,
    redraw: () => draw(),
    onPlay: (x, y) => humanMove(Common.toCoord(x, y, 15)),
    cellText: (x, y) => {
      if (!view) return "";
      const i = y * 15 + x;
      const c = view.state.board[i];
      return (c ? `${NAME[c]} 돌` : "빈 점") + (view.state.last === i ? ", 직전 수" : "");
    },
  });

  function draw() {
    grid.drawBoard(STARS);
    if (view) {
      const st = view.state, n = st.size;
      st.board.forEach((c, i) => { if (c) grid.drawStone(i % n, Math.floor(i / n), c); });
      if (st.win_line.length) {
        const ctx = grid.ctx;
        ctx.strokeStyle = "#c62828"; ctx.lineWidth = 4;
        for (const i of st.win_line) {
          ctx.beginPath(); ctx.arc(grid.pos(i % n), grid.pos(Math.floor(i / n)), grid.step * 0.3, 0, Math.PI * 2); ctx.stroke();
        }
      } else if (st.last != null) {
        grid.drawMarker(st.last % n, Math.floor(st.last / n), st.board[st.last]);
      }
      if (hover && humanTurn() && !st.board[hover.y * n + hover.x]) grid.drawStone(hover.x, hover.y, view.humanColor, 0.4);
    }
    cursor.draw(grid);
  }

  function render(thinking) {
    draw();
    $("omok-undo").disabled = !view || busy || !view.canUndo;
    $("omok-coord").disabled = !humanTurn();
    $("omok-read").disabled = !view;
    const info = $("omok-info");
    if (!view) { info.textContent = "새 게임을 눌러 시작하세요."; return; }
    let text = `나: ${NAME[view.humanColor]} · ${view.state.history.length}수\n`;
    if (view.result) text += `결과: ${view.result}`;
    else if (thinking) text += thinking;
    else text += humanTurn() ? "내 차례입니다." : "AI 차례입니다.";
    info.innerHTML = Common.escape(text).replace(/(중)$/, '<span class="thinking">$1</span>');
  }

  async function call(path, body, thinking) {
    busy = true;
    render(thinking);
    try {
      view = await Common.api(`/api/omok/${view.id}/${path}`, body);
      return true;
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      return false;
    } finally {
      busy = false;
      render();
    }
  }

  async function aiTurn() {
    if (view.over || view.state.to_play === view.humanColor) return;
    if (await call("ai", { think: Common.think() }, "AI가 생각하는 중")) {
      Common.logAi(LOG, view.ai, NAME[3 - view.humanColor]);
      for (const t of view.ai.threats || []) Common.log(LOG, `주의 — ${Common.escape(t)}`);
      if (view.result) Common.log(LOG, `<b>결과: ${Common.escape(view.result)}</b>`);
    }
  }

  async function humanMove(move) {
    if (!humanTurn()) return;
    if (await call("play", { move })) {
      Common.log(LOG, `나 ${NAME[view.humanColor]} ${Common.escape(move.toUpperCase())}`);
      if (view.result) Common.log(LOG, `<b>결과: ${Common.escape(view.result)}</b>`);
      await aiTurn();
    }
  }

  $("omok-new").addEventListener("click", async () => {
    if (busy) return;
    try {
      view = await Common.api("/api/omok/new", { humanColor: $("omok-color").value === "B" ? 1 : 2 });
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      return;
    }
    $(LOG).innerHTML = "";
    Common.log(LOG, `새 게임: 나는 ${NAME[view.humanColor]}입니다.`);
    render();
    await aiTurn();
  });

  canvas.addEventListener("pointermove", e => { hover = grid.pick(e); draw(); });
  canvas.addEventListener("pointerleave", () => { hover = null; draw(); });
  canvas.addEventListener("click", e => {
    const p = grid.pick(e);
    if (p) humanMove(Common.toCoord(p.x, p.y, 15));
  });
  $("omok-form").addEventListener("submit", e => {
    e.preventDefault();
    const v = $("omok-coord").value.trim();
    if (!v) return;
    if (!Common.fromCoord(v, 15)) {
      Common.log(LOG, `"${Common.escape(v)}"는 올바른 좌표가 아닙니다. 예: H8`, true);
      return;
    }
    $("omok-coord").value = "";
    humanMove(v).then(() => $("omok-coord").focus());
  });
  $("omok-undo").addEventListener("click", async () => {
    if (await call("undo", {})) Common.log(LOG, "한 수 물렀습니다.");
  });
  $("omok-read").addEventListener("click", () => {
    if (view) Common.announce(Common.describeBoard(view.state.board, 15));
  });

  render();
})();
