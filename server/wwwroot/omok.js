// 오목 화면: 승패 판정은 서버(Rust 엔진), 수 선택은 Claude.
"use strict";
(() => {
  const $ = id => document.getElementById(id);
  const canvas = $("omok-canvas");
  const grid = new Common.Grid(canvas, 15);
  const STARS = [[3, 3], [11, 3], [7, 7], [3, 11], [11, 11]];
  const LOG = "omok-log";

  let view = null, busy = false, hover = null;
  const humanTurn = () => view && !view.over && !busy && view.state.to_play === view.humanColor;

  function draw() {
    grid.drawBoard(STARS);
    if (!view) return;
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

  function render(thinking) {
    draw();
    $("omok-undo").disabled = !view || busy || !view.canUndo;
    const info = $("omok-info");
    if (!view) { info.textContent = "새 게임을 눌러 시작하세요."; return; }
    let text = `나: ${view.humanColor === 1 ? "흑" : "백"} · ${view.state.history.length}수\n`;
    if (view.result) text += `🏁 ${view.result}`;
    else if (thinking) text += thinking;
    else text += humanTurn() ? "내 차례입니다." : "Claude 차례입니다.";
    info.innerHTML = Common.escape(text).replace(/(Claude[^\n]*중)$/, '<span class="thinking">$1</span>');
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
    if (await call("ai", { effort: Common.effort() }, "Claude가 생각하는 중")) Common.logAi(LOG, view.ai);
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
    render();
    await aiTurn();
  });

  canvas.addEventListener("pointermove", e => { hover = grid.pick(e); draw(); });
  canvas.addEventListener("pointerleave", () => { hover = null; draw(); });
  canvas.addEventListener("click", async e => {
    const p = grid.pick(e);
    if (!p || !humanTurn()) return;
    if (await call("play", { move: Common.toCoord(p.x, p.y, 15) })) await aiTurn();
  });
  $("omok-undo").addEventListener("click", () => call("undo", {}));

  render();
})();
