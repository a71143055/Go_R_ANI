// 바둑 화면: 규칙 판정은 서버(Rust 엔진), 수 선택은 Claude. 여기서는 그리기와 입력만 담당한다.
"use strict";
(() => {
  const $ = id => document.getElementById(id);
  const canvas = $("go-canvas");
  const grid = new Common.Grid(canvas, 9);
  const STARS = {
    9: [[2, 2], [6, 2], [4, 4], [2, 6], [6, 6]],
    13: [[3, 3], [9, 3], [6, 6], [3, 9], [9, 9]],
    19: [[3, 3], [9, 3], [15, 3], [3, 9], [9, 9], [15, 9], [3, 15], [9, 15], [15, 15]],
  };
  const LOG = "go-log";

  let view = null;       // 서버가 돌려준 게임 상태
  let busy = false;
  let hover = null;
  let peekBoard = null;  // 맹기 엿보기 중인 실제 판
  let flashUntil = 0;    // 맹기 모드에서 직전 수를 잠깐 보여 주는 시각

  const humanTurn = () => view && !view.over && !busy && view.state.to_play === view.humanColor;
  const lastMover = st => {
    const h = st.history[st.history.length - 1];
    return h && h[0] === "B" ? 1 : 2;
  };

  function draw() {
    const n = view ? view.state.size : parseInt($("go-size").value, 10);
    grid.setSize(n);
    grid.drawBoard(STARS[n] || []);
    if (!view) return;
    const st = view.state;
    const board = peekBoard || st.board;
    const score = view.score;
    const dead = new Set((score?.dead || []).map(c => { const p = Common.fromCoord(c, n); return p ? p.y * n + p.x : -1; }));

    for (let i = 0; i < board.length; i++) {
      if (!board[i]) continue;
      grid.drawStone(i % n, Math.floor(i / n), board[i], dead.has(i) ? 0.45 : 1);
      if (dead.has(i)) grid.drawCross(i % n, Math.floor(i / n));
    }
    // 계가 결과: 집 표시
    if (score?.ownership) {
      const ctx = grid.ctx, d = grid.step * 0.18;
      score.ownership.forEach((o, i) => {
        if (!o || (board[i] && !dead.has(i))) return;
        ctx.fillStyle = o === 1 ? "rgba(0,0,0,.75)" : "rgba(255,255,255,.9)";
        ctx.fillRect(grid.pos(i % n) - d, grid.pos(Math.floor(i / n)) - d, d * 2, d * 2);
      });
    }
    // 직전 수
    if (st.last != null) {
      const x = st.last % n, y = Math.floor(st.last / n), c = lastMover(st);
      if (view.hidden && !peekBoard) {
        if (Date.now() < flashUntil) { grid.drawStone(x, y, c, 0.9); grid.drawMarker(x, y, c); }
      } else if (board[st.last]) {
        grid.drawMarker(x, y, board[st.last]);
      }
    }
    // 마우스 위치 미리보기
    if (hover && humanTurn() && !peekBoard && !board[hover.y * n + hover.x]) {
      grid.drawStone(hover.x, hover.y, view.humanColor, 0.4);
    }
  }

  function flashLast() {
    if (!view?.hidden || view.state.last == null) return;
    flashUntil = Date.now() + 1500;
    setTimeout(draw, 1600);
  }

  function render(thinking) {
    draw();
    const info = $("go-info");
    $("go-pass").disabled = $("go-resign").disabled = !humanTurn();
    $("go-undo").disabled = !view || busy || !view.canUndo;
    $("go-peek").hidden = !view || view.blind === "off" || view.over;
    $("go-moves-wrap").hidden = !view || view.blind === "off";
    if (!view) { info.textContent = "새 대국을 눌러 시작하세요."; return; }

    const st = view.state;
    const me = view.humanColor === 1 ? "흑" : "백";
    const mode = { off: "", human: " · 맹기(나만)", both: " · 맹기(Claude도)" }[view.blind];
    let text = `${st.size}줄 · 나: ${me}${mode}\n따낸 돌 — 흑 ${st.captures[0]} · 백 ${st.captures[1]}`;
    if (view.blind !== "off") text += ` · 엿보기 ${view.peeks}회`;
    text += "\n";
    if (view.result) text += `🏁 ${view.result}`;
    else if (view.over) text += "종국 — 계가 중";
    else if (thinking) text += thinking;
    else text += humanTurn() ? "내 차례입니다." : "Claude 차례입니다.";
    info.innerHTML = Common.escape(text).replace(/(Claude[^\n]*중)$/, '<span class="thinking">$1</span>');

    if (view.blind !== "off") {
      const h = st.history;
      $("go-moves").innerHTML = h.map((m, i) =>
        `<span class="${i === h.length - 1 ? "last" : ""}">${i + 1}.${Common.escape(m)}</span>`).join(" ") || "(아직 없음)";
      $("go-moves").scrollTop = $("go-moves").scrollHeight;
    }
  }

  async function call(path, body, thinking) {
    busy = true;
    render(thinking);
    try {
      view = await Common.api(`/api/go/${view.id}/${path}`, body);
      return true;
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      return false;
    } finally {
      busy = false;
      render();
    }
  }

  async function afterMove() {
    if (view.over && !view.result) {
      if (await call("score", { effort: Common.effort() }, "Claude가 사석을 판정하는 중")) Common.logAi(LOG, view.ai);
      return;
    }
    if (!view.over && view.state.to_play !== view.humanColor) {
      if (await call("ai", { effort: Common.effort() }, "Claude가 생각하는 중")) {
        Common.logAi(LOG, view.ai);
        flashLast();
        if (view.over && !view.result) await afterMove();
      }
    }
  }

  async function humanMove(move) {
    if (!humanTurn()) return;
    if (await call("play", { move })) {
      flashLast();
      await afterMove();
    }
  }

  $("go-new").addEventListener("click", async () => {
    if (busy) return;
    try {
      view = await Common.api("/api/go/new", {
        size: parseInt($("go-size").value, 10),
        komi: parseFloat($("go-komi").value) || 0,
        humanColor: $("go-color").value === "B" ? 1 : 2,
        blind: $("go-blind").value,
      });
    } catch (e) {
      Common.log(LOG, Common.escape(e.message), true);
      return;
    }
    $(LOG).innerHTML = "";
    if (view.blind !== "off") Common.log(LOG, "맹기 대국입니다. 판에는 돌이 보이지 않습니다. 직전 수는 1.5초 동안만 표시됩니다.");
    render();
    await afterMove();
  });

  canvas.addEventListener("pointermove", e => { hover = grid.pick(e); draw(); });
  canvas.addEventListener("pointerleave", () => { hover = null; draw(); });
  canvas.addEventListener("click", e => {
    const p = grid.pick(e);
    if (p && view) humanMove(Common.toCoord(p.x, p.y, view.state.size));
  });
  $("go-pass").addEventListener("click", () => humanMove("pass"));
  $("go-undo").addEventListener("click", () => call("undo", {}));
  $("go-resign").addEventListener("click", () => { if (confirm("기권하시겠습니까?")) call("resign", {}); });
  $("go-size").addEventListener("change", () => { if (!view) draw(); });

  // 맹기 엿보기: 누르고 있는 동안만 실제 판을 보여 준다
  const peekStart = async e => {
    e.preventDefault();
    if (!view || busy) return;
    try {
      const r = await Common.api(`/api/go/${view.id}/peek`);
      view.peeks = r.peeks;
      peekBoard = r.board;
      render();
    } catch (err) {
      Common.log(LOG, Common.escape(err.message), true);
    }
  };
  const peekEnd = () => { if (peekBoard) { peekBoard = null; render(); } };
  $("go-peek").addEventListener("pointerdown", peekStart);
  ["pointerup", "pointerleave", "pointercancel"].forEach(ev => $("go-peek").addEventListener(ev, peekEnd));

  render();
})();
